// PERMUTATION STATE — playable client (Game Design V5).
// You are a member of one nation. Flow: select on the map → the inspector
// explains it → every option shows cost, time and, when blocked, the engine's
// own reason → add to this tick's orders. Orders of offices you hold are
// sealed and sent; orders of other offices become proposals (献策) to their
// officers. The nation plaza holds the government: offices, elections,
// proposals and recalls.
import { WorldMap, drawMinimap, key } from './map.mjs';
import * as T from './i18n.mjs';
import * as V from './verify.mjs';

const $ = s => document.querySelector(s);
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const fmt = n => Number(n ?? 0).toLocaleString('ja-JP');
// Member token: this browser's membership of one nation (POST /api/join, or
// /api/claim in chain mode). Kept per origin; storage can be unavailable, so
// every access is guarded.
const SEAT_KEY = 'ps-member-token';
const seatStore = {
  get() { try { return localStorage.getItem(SEAT_KEY); } catch { return null; } },
  set(v) { try { if (v) localStorage.setItem(SEAT_KEY, v); else localStorage.removeItem(SEAT_KEY); } catch { /* private mode */ } },
};
let seatToken = null;
const auth = () => (seatToken ? { 'X-Member-Token': seatToken } : {});
const api = {
  get: p => fetch(p, { cache: 'no-store', headers: auth() }).then(r => { if (!r.ok) throw new Error(r.status); return r.json(); }),
  post: (p, b) => fetch(p, { method: 'POST', headers: { 'Content-Type': 'application/json', ...auth() }, body: JSON.stringify(b) }).then(r => r.json()),
};
const hexDist = (a, b) => (Math.abs(a.q - b.q) + Math.abs(a.r - b.r) + Math.abs(a.q + a.r - b.q - b.r)) / 2;

const S = {
  map: null, view: null, me: 0, member: null, online: false,
  tile: null, unit: null, drawer: null, lens: 'normal',
  drafts: [], committedJson: '[]', lastTick: null, committing: false,
  unitPreview: null, cityPreview: null, research: null, diplo: {},
  nextIndex: 0, summaryOpen: true, summaryCollapsed: false, summaryPinned: false, marketQuote: null, exchangeStep: 0,
};

// ================================================================== setup
const map = new WorldMap($('#world-map'), {
  onSelect: id => selectTile(id),
  onHover: id => hoverTile(id),
  onMove: id => quickMove(id),
});

function setHtml(el, html) { if (el.__html !== html) { el.innerHTML = html; el.__html = html; } }
function toast(text, kind = '', action = null) {
  const box = $('#notifications'); const el = document.createElement('div');
  el.className = `toast ${kind}`; el.innerHTML = `<span>${text}</span>`;
  if (action) { const b = document.createElement('button'); b.textContent = action.label; b.onclick = () => { action.run(); el.remove(); }; el.appendChild(b); }
  box.prepend(el); while (box.children.length > 3) box.lastChild.remove();
  setTimeout(() => el.remove(), kind === 'error' || kind === 'war' ? 8000 : 4500);
}

// ================================================================== state
const ROLE_JA = { General: '将軍', Steward: '内政官', Science: '科学官', Diplomat: '外交官' };
const ROLE_GLYPH = { General: '⚔', Steward: '⌂', Science: '✧', Diplomat: '✉' };
const ROLES = ['General', 'Steward', 'Science', 'Diplomat'];
const PATH_JA = ['覇権', '繁栄', '科学', '協調'];
const KIND_JA = { human: '人間', agent: 'AI', undeclared: '未申告' };
const usdc = x => (Number(x ?? 0) / 1e6).toLocaleString('ja-JP', { maximumFractionDigits: 2 });

/**
 * Registration (V5 §4): choose a nation, a name, and the offices to stand
 * for; in chain mode, take over a member the gateway registered for you.
 * Resolves with the member token.
 */
function pickSeat(info) {
  const dlg = $('#seats');
  S.lobbyPick ??= { civ: null, stand: ['General', 'Steward'] };
  const render = (info, error = '') => {
    $('#seat-mode').textContent = info.mode === 'chain' ? 'オンチェーンのシーズン（MagicBlock ER）' : 'ローカルのシーズン（テスト用USDC）';
    if (info.mode === 'chain') {
      const claimable = info.members.filter(m => m.claimable);
      setHtml($('#seat-list'), (claimable.map(m => `<li class="seat human"><span class="swatch" style="background:${T.CIV_COLORS[m.civ]}"></span><span class="who"><b>${esc(m.name)}</b><small>${esc(T.civName(info.nations[m.civ]?.name))}の国民（ゲートウェイが登録済み）</small></span><button class="btn primary" type="button" data-claim-member="${m.id}">この国民で遊ぶ</button></li>`).join('')
        || '<li class="seat-none">いま引き継げる国民はいません。AI エージェントは x402 で参加できます（llms.txt）。<a class="btn primary" href="spectate.html">観戦する →</a></li>') + (error ? `<li class="seat-error">${esc(error)}</li>` : ''));
      return;
    }
    const pick = S.lobbyPick;
    const fee = usdc(info.entryFee);
    setHtml($('#seat-list'), info.nations.map(n => `<li class="seat nation ${pick.civ === n.civ ? 'picked' : ''}" data-pick-civ="${n.civ}">
        <span class="swatch" style="background:${T.CIV_COLORS[n.civ]}"></span>
        <span class="who"><b>${esc(T.civName(n.name))}</b><small>国民 ${n.members}人${n.perMember ? ` · 今の1人あたりの見込み ${usdc(n.perMember)} USDC` : ''}</small></span>
        <span class="meta">${pick.civ === n.civ ? '選択中 ✓' : 'えらぶ'}</span></li>`).join('')
      + `<li class="seat-form"><label class="field">名前<input id="join-name" maxlength="24" placeholder="あなたの名前" value="${esc(S.joinName || '')}"></label>
         <div class="field">立候補する役職（2つまで）<div class="row">${ROLES.map(r => `<button type="button" class="btn ${pick.stand.includes(r) ? 'primary' : ''}" data-pick-stand="${r}">${ROLE_GLYPH[r]} ${ROLE_JA[r]}</button>`).join('')}</div></div>
         <p class="desc">参加費は全員同じ ${fee} USDC（80%が賞金プール、20%が運営）。国民がいない役職はAIの代行が務めます。</p>
         <button class="btn primary wide" type="button" id="join-btn" ${pick.civ === null ? 'disabled' : ''}>${pick.civ === null ? '国を選んでください' : `${esc(T.civName(info.nations[pick.civ].name))}の国民になる`}</button></li>`
      + (error ? `<li class="seat-error">${esc(error)}</li>` : ''));
  };
  render(info);
  const refresh = setInterval(async () => { if (dlg.open) { info = await api.get('/api/lobby').catch(() => info); render(info); } }, 3000);
  dlg.showModal();
  return new Promise(resolve => {
    dlg.onclick = async e => {
      const t = e.target.closest('[data-pick-civ], [data-pick-stand], #join-btn, [data-claim-member]');
      if (!t) return;
      if (t.dataset.pickCiv !== undefined) { S.lobbyPick.civ = +t.dataset.pickCiv; S.joinName = $('#join-name')?.value; return render(info); }
      if (t.dataset.pickStand) {
        const st = S.lobbyPick.stand, r = t.dataset.pickStand;
        S.lobbyPick.stand = st.includes(r) ? st.filter(x => x !== r) : [...st, r].slice(-2);
        S.joinName = $('#join-name')?.value; return render(info);
      }
      let res;
      if (t.dataset.claimMember !== undefined) res = await api.post('/api/claim', { member: +t.dataset.claimMember }).catch(() => ({ ok: false, error: 'network' }));
      else res = await api.post('/api/join', { civ: S.lobbyPick.civ, name: $('#join-name').value, kind: 'human', stand: S.lobbyPick.stand }).catch(() => ({ ok: false, error: 'network' }));
      if (res.ok) { clearInterval(refresh); seatStore.set(res.token); dlg.close(); resolve(res.token); return; }
      render(await api.get('/api/lobby'), res.error);
    };
    dlg.oncancel = e => e.preventDefault(); // membership (or spectating) is required
  });
}

/** Who else is in this season, from the member list. */
function othersText(v) {
  const ms = v?.members || [];
  const n = h => ms.filter(x => x.host === h).length;
  return `この季節の国民は${ms.length}人（人間${n('human')}・AI${n('ai') + n('external')}）。国民のいない役職はAIの代行が務めます。`;
}

/** Help text that matches who is actually in this season. */
function describeSeats(v) {
  $('#help-desc').textContent = `${v.civs.length}つの国が同じ地図を共有しています。${othersText(v)}${v.tickSeconds}秒ごとの「ティック」で、全ての国の命令が同時に解決されます。`;
  if (v.chain) $('#help-mode').textContent = 'このシーズンはオンチェーンです（MagicBlock ER）。時計は止まりません。「手番を終える」で担当の命令に署名して送ります。';
}

async function boot() {
  const params = new URLSearchParams(location.search);
  if (params.has('spectate')) { location.replace('spectate.html'); return; }
  if (params.get('token')) { seatStore.set(params.get('token')); history.replaceState(null, '', location.pathname); }
  seatToken = seatStore.get();
  // ?watch=N: read-only view through civ N's fog (for review; no seat needed).
  if (params.has('watch')) { S.watch = +params.get('watch'); seatToken = null; }
  try {
    const lobby = await api.get('/api/lobby');
    if (lobby.you === null && S.watch === undefined) { seatStore.set(null); seatToken = null; seatToken = await pickSeat(lobby); }
    S.map = await api.get('/api/map');
    map.setMap(S.map);
    await poll();
    describeSeats(S.view);
    $('#loading').hidden = true;
    if (S.view.phase === 'lobby' || (S.view.paused && S.view.tick === 0)) $('#help').showModal();
  } catch (e) {
    $('#loading-text').textContent = 'ゲームサーバーに接続できません。`cargo run --release --bin play` を起動してから再読み込みしてください。';
  }
  setInterval(poll, 700);
  setInterval(renderClock, 250);
}

async function poll() {
  let v;
  try { v = await api.get(`/api/state${S.watch !== undefined ? `?civ=${S.watch}` : ''}`); S.online = true; } catch { S.online = false; renderClock(); return; }
  // The server no longer knows this browser's member token (it restarted):
  // back to the lobby to claim or join again, instead of rendering a view
  // that belongs to no nation.
  if (seatToken && S.watch === undefined && v.viewer !== 'member') {
    seatStore.set(null);
    toast('サーバーが再起動したため、国民として入り直してください。', 'error');
    setTimeout(() => location.reload(), 1200);
    return;
  }
  const newTick = S.lastTick !== null && v.tick !== S.lastTick;
  const prev = S.view;
  S.view = v; S.me = v.me; S.member = v.member?.id ?? null;
  const ms = v.members || [];
  $('.prototype-label').textContent = `${v.chain ? 'オンチェーン · MagicBlock ER' : 'ローカル'} · 国民${ms.length}人（人間${ms.filter(m => m.host === 'human').length}） · 同じ霧の中で判断 · USDCはテスト用`;
  if (S.lastTick === null && v.member?.committed) {
    const orders = v.member.committed.flatMap(c => c.orders);
    if (orders.length) { S.drafts = orders.map(describe); S.committedJson = JSON.stringify(orders); map.setDrafts(S.drafts); }
  }
  if (newTick) onNewTick(v, prev);
  S.lastTick = v.tick;
  map.setView(v, v.me);
  // Auto-commit a dirty draft just before the deadline (never lose orders silently).
  if (!v.paused && isDirty() && v.secondsLeft < 3 && !S.committing) commit(true);
  renderAll();
}

function onNewTick(v, prev) {
  S.drafts = []; S.committedJson = '[]'; map.setDrafts([]); S.adopt = {};
  if (!v.gov?.voteOpen) S.myVotes = {};
  for (const k of v.skipped || []) toast(`見送られた命令：${ROLE_JA[k.role] || ''}${k.index === null ? 'の命令全体' : `の${k.index + 1}件目`}（${T.blockedText({ code: k.reason })}）`, 'error');
  for (const line of v.lastSummary || []) {
    const [kind, text] = T.chronicleText(line);
    if ((kind === 'era' || kind === 'gov' || kind === 'recall') && text.includes(civN(v.me))) toast(text, kind === 'era' ? 'good' : '');
  }
  $('#rationale').value = ''; S.committedRationale = ''; S.digest = null;
  if (S.drawer === 'decisions') loadDecisions();
  S.digest = prev ? digest(prev, v) : [];
  S.unitPreview = null; S.cityPreview = null; S.research = null; S.diplo = {};
  S.summaryOpen = true; S.summaryCollapsed = false; S.summaryPinned = false;
  for (const line of v.lastSummary || []) {
    const [kind, text] = T.chronicleText(line);
    const mine = text.includes(civN(v.me));
    if ((kind === 'war' || kind === 'capture') && mine) toast(text, 'war');
  }
  for (const p of v.proposals.filter(p => p.to === v.me && p.tick === v.tick - 1)) {
    toast(`${civN(p.from)}から${({ Peace: '講和', Nap: '不可侵条約', Alliance: '同盟' })[p.kind]}の申し入れ（ティック${p.expires}まで有効）`, '', { label: '外交を開く', run: () => openDrawer('diplomacy') });
  }
  refreshSelection();
}

const civ = id => S.view?.civs?.[id];
const civN = id => T.civName(civ(id)?.name ?? S.map?.civNames?.[id] ?? `#${id}`);
const myUnits = () => S.view.units.filter(u => u.owner === S.me);
const myCities = () => S.view.cities.filter(c => c.owner === S.me);
const tileIndex = id => map.tiles.get(id)?.index;
const ownerOf = id => { const c = S.view.owners[tileIndex(id)]; return c && c !== '.' ? parseInt(c, 36) : null; };

// ================================================================== drafts
const rationaleText = () => ($('#rationale')?.value || '').trim();
function isDirty() { return JSON.stringify(S.drafts.map(d => d.dto)) !== S.committedJson || rationaleText() !== (S.committedRationale || ''); }
/** Offices this member holds (role names). */
function held() { return (S.view?.gov?.offices || []).filter(o => o.holder && o.holder.id === S.member).map(o => o.role); }
const FREE = new Set(['ExchangeOrder', 'RevealRationale', 'ConsentWar', 'ConsentSpend']);
const costOf = dto => (FREE.has(dto.type) ? 0 : 1);
/** The office an order belongs to (V5 §5.1); settlers are the steward's. */
function officeOf(dto) {
  if (dto.type === 'MoveUnit' || (dto.type === 'SetStanding' && dto.target.kind === 'Unit')) {
    const id = dto.type === 'MoveUnit' ? dto.unit : dto.target.id;
    return S.view?.units.find(u => u.id === id)?.type === 'Settler' ? 'Steward' : 'General';
  }
  return ({ Attack: 'General', Raze: 'General', FoundCity: 'Steward', SetQueue: 'Steward', SetFocus: 'Steward', Purchase: 'Steward', SetStanding: 'Steward',
    SetResearch: 'Science', ConsentWar: 'General', ConsentSpend: 'General' })[dto.type] || 'Diplomat';
}
function officeInfo(role) { return (S.view?.economy.offices || []).find(o => o.role === role) || { budget: 0, bank: 0, spendable: 0 }; }
function spendable(role) { return role ? officeInfo(role).spendable : held().reduce((a, r) => a + officeInfo(r).spendable, 0); }
function used(role) { return S.drafts.filter(d => !role || d.office === role).reduce((a, d) => a + costOf(d.dto), 0); }

/** Identity of the slot an order occupies: one order per unit, per city field, per diplomatic target. */
function slotOf(dto) {
  switch (dto.type) {
    case 'MoveUnit': case 'FoundCity': return `u${dto.unit ?? dto.settler}`;
    case 'Attack': return `u${dto.army}`;
    case 'SetQueue': return `q${dto.city}`; case 'SetFocus': return `f${dto.city}`; case 'Purchase': return `p${dto.city}`;
    case 'SetResearch': return 'research';
    case 'SendEnvoy': return `e${dto.cityState}`;
    case 'SetStanding': return dto.target.kind === 'Unit' ? `u${dto.target.id}` : `cs${dto.target.id}:${dto.rule.kind === 'Clear' ? 'all' : dto.rule.kind}`;
    case 'MarketTrade': case 'ExchangeOrder': return `m${Math.random()}`;
    default: return `${dto.type}${dto.civ ?? ''}`;
  }
}
function addDraft(dto) {
  if (S.watch !== undefined) { toast('観戦中は命令を出せません。', 'error'); return false; }
  const d = describe(dto);
  const slot = slotOf(dto);
  const others = S.drafts.filter(x => slotOf(x.dto) !== slot);
  const replacing = others.length !== S.drafts.length;
  const mine = held().includes(d.office);
  if (mine && !replacing && used(d.office) + costOf(dto) > spendable(d.office)) {
    toast(`${ROLE_JA[d.office]}の命令の枠が足りません（${used(d.office)}/${spendable(d.office)}）。先に他の命令を取り消してください。`, 'error'); return false;
  }
  S.drafts = [...others, d];
  renderDock(); renderTop(); map.setDrafts(S.drafts); renderNext();
  if (!mine && !S.hintedPropose) { S.hintedPropose = true; toast(`${ROLE_JA[d.office]}はあなたの担当ではないので、この命令は「献策」になります。確定すると${ROLE_JA[d.office]}に提案され、採用されれば功績を半分ずつ分けます。`); }
  else if (!S.hintedDock) { S.hintedDock = true; toast('命令は下のドックに貯まります。「確定する」（Ctrl+Enter）で封印して送信、締切3秒前には自動で確定します。'); }
  const chip = document.querySelector(`#chips [data-chip="${S.drafts.length - 1}"]`);
  if (chip) { chip.classList.add('pulse'); setTimeout(() => chip.classList.remove('pulse'), 900); }
  return true;
}
function removeDraft(i) { S.drafts.splice(i, 1); renderDock(); renderTop(); map.setDrafts(S.drafts); renderNext(); renderInspector(); }

/** Human label, glyph, office and map hints for an order DTO. */
function describe(dto) {
  const d = describeOrder(dto);
  d.office = officeOf(dto);
  d.proposal = S.view ? !held().includes(d.office) : false;
  return d;
}
function describeOrder(dto) {
  const u = id => S.view?.units.find(x => x.id === id);
  const city = id => S.view?.cities.find(c => c.id === id);
  const at = x => x ? key(x.q, x.r) : null; const at_ = at;
  switch (dto.type) {
    case 'MoveUnit': { const x = u(dto.unit); const last = dto.path[dto.path.length - 1];
      return { dto, glyph: '→', label: `${T.UNIT[x?.type] || '部隊'}を移動（${dto.path.length}マス）`, focus: key(last[0], last[1]), from: x, path: dto.path }; }
    case 'Attack': { const x = u(dto.army); const t = dto.target; let pos = null, what = '';
      if (t.kind === 'Unit') { const y = u(t.id); pos = y; what = `${civN(y?.owner)}の${T.UNIT[y?.type] || '部隊'}`; }
      if (t.kind === 'City') { const y = city(t.id); pos = y; what = T.cityName(t.id); }
      if (t.kind === 'CityState') { const y = S.view.cityStates.find(c => c.id === t.id); pos = y; what = `都市国家${t.id + 1}`; }
      return { dto, glyph: '⚔', label: `${T.UNIT[x?.type] || '部隊'}で${what}を攻撃`, focus: at(pos), from: x, attackAt: pos }; }
    case 'FoundCity': return { dto, glyph: '⌂', label: '開拓者が都市を建設', focus: at(u(dto.settler)) };
    case 'SetQueue': return { dto, glyph: '▤', label: `${T.cityName(dto.city)}：${dto.items.map(T.itemName).join(' → ') || '生産なし'}`, focus: at(city(dto.city)) };
    case 'SetFocus': return { dto, glyph: '◐', label: `${T.cityName(dto.city)}の方針：${T.FOCUS[dto.focus]}`, focus: at(city(dto.city)) };
    case 'Purchase': return { dto, glyph: '◆', label: `${T.cityName(dto.city)}で生産を購入（${dto.gold}金）`, focus: at(city(dto.city)) };
    case 'SetResearch': return { dto, glyph: '✧', label: `研究：${dto.techs.map(t => T.TECH[t]).join(' → ')}` };
    case 'SetStanding': {
      const who = dto.target.kind === 'Unit' ? `${T.UNIT[u(dto.target.id)?.type] || '部隊'}` : T.cityName(dto.target.id);
      const at = dto.target.kind === 'Unit' ? at_(u(dto.target.id)) : at_(city(dto.target.id));
      return { dto, glyph: T.STANDING_GLYPH[dto.rule.kind] || '⚙', label: `${who}：${T.standingText(dto.rule)}`, focus: at };
    }
    case 'DeclareWar': return { dto, glyph: '⚔', label: `${civN(dto.civ)}に宣戦` };
    case 'ProposePeace': return { dto, glyph: '☮', label: `${civN(dto.civ)}に講和を申し入れ` };
    case 'AcceptPeace': return { dto, glyph: '☮', label: `${civN(dto.civ)}の講和を受諾` };
    case 'ProposeNap': return { dto, glyph: '✉', label: `${civN(dto.civ)}に不可侵条約（保証金${dto.bond}）` };
    case 'AcceptNap': return { dto, glyph: '✉', label: `${civN(dto.civ)}の不可侵条約を受諾` };
    case 'BreakNap': return { dto, glyph: '✕', label: `${civN(dto.civ)}との条約を破棄（宣戦）` };
    case 'ProposeAlliance': return { dto, glyph: '⚭', label: `${civN(dto.civ)}に同盟を申し入れ` };
    case 'AcceptAlliance': return { dto, glyph: '⚭', label: `${civN(dto.civ)}の同盟に加わる` };
    case 'LeaveAlliance': return { dto, glyph: '⚭', label: '同盟から離脱' };
    case 'SendEnvoy': return { dto, glyph: '✉', label: `都市国家${dto.cityState + 1}に使節（影響力${dto.influence}）`, focus: at(S.view.cityStates.find(c => c.id === dto.cityState)) };
    case 'MarketTrade': return { dto, glyph: '⇄', label: `金の市場：${T.RESOURCE[dto.good.kind]}を${dto.amount}${dto.side === 'Buy' ? '購入' : '売却'}` };
    case 'ExchangeOrder': return { dto, glyph: '$', usdc: true, label: `USDC取引所：${goodName(dto.good)}${dto.amount}を${dto.side === 'Buy' ? '買い' : '売り'} @${(dto.price / 1e6).toFixed(2)}` };
    case 'Raze': return { dto, glyph: '✕', label: `${T.cityName(dto.city)}を破壊` };
    case 'ConsentWar': return { dto, glyph: '⚖', label: `${civN(dto.civ)}への宣戦に同意` };
    case 'ConsentSpend': return { dto, glyph: '⚖', label: `国庫から${usdc(dto.usdc)} USDCまでの支出に同意` };
    default: return { dto, glyph: '•', label: dto.type };
  }
}
const goodName = g => ({ Gold: '金', Iron: '鉄', Horses: '馬', Food: '食料', Production: '生産' })[g.kind];

/**
 * Seal and send the orders of the offices held, and turn the rest into
 * proposals to their offices (one proposal per office, V5 §5.4).
 */
async function commit(auto = false) {
  if (S.committing) return;
  S.committing = true;
  const mine = S.drafts.filter(d => !d.proposal), props = S.drafts.filter(d => d.proposal);
  const orders = mine.map(d => d.dto);
  const rationale = rationaleText();
  const adopt = {};
  for (const [role, ids] of Object.entries(S.adopt || {})) if (ids.length && held().includes(role)) adopt[role] = ids;
  let ok = true, msg = [];
  // Proposals first: once every office's batch is in, the tick closes and
  // later governance actions wait for the next tick.
  const byRole = {};
  for (const d of props) (byRole[d.office] ||= []).push(d.dto);
  for (const [role, list] of Object.entries(byRole)) {
    for (let i = 0; i < list.length; i += 4) {
      const res = await api.post('/api/gov', { action: { type: 'Propose', role, orders: list.slice(i, i + 4) } }).catch(() => ({ ok: false, error: 'network' }));
      if (res.ok) msg.push(`${ROLE_JA[role]}に献策（${list.slice(i, i + 4).length}件）`);
      else { ok = false; toast(`献策できませんでした：${translateError(res.error)}`, 'error'); }
    }
  }
  if (held().length) {
    const res = await api.post('/api/orders', { orders, rationale, adopt }).catch(() => ({ ok: false, error: 'network' }));
    const nAdopt = Object.values(adopt).reduce((n, l) => n + l.length, 0);
    if (res.ok) { S.digest = res.offices; msg.push(orders.length ? `命令${orders.length}件を封印して確定（${res.offices.map(o => ROLE_JA[o.role]).join('・')}）` : nAdopt ? '' : '担当の命令なしで確定'); if (nAdopt) msg.push(`献策${nAdopt}件の採用を確定`); msg = msg.filter(Boolean); }
    else { ok = false; toast(`確定できませんでした：${translateError(res.error)}`, 'error'); }
  }
  S.committing = false;
  if (ok) {
    S.committedJson = JSON.stringify(S.drafts.map(d => d.dto));
    S.committedRationale = rationale;
    if (props.length) { S.drafts = mine; S.committedJson = JSON.stringify(mine.map(d => d.dto)); map.setDrafts(S.drafts); }
    if (!auto) toast(`${msg.join(' / ') || '確定しました'}。締切（全役職の確定）まで変更できます。`);
    else if (S.drafts.length) toast('締切が近いため、下書きを自動で確定しました。');
  }
  renderDock();
}
function translateError(e = '') {
  let m;
  if ((m = e.match(/orders cost (\d+), only (\d+) spendable/))) return `命令の枠が足りません（必要${m[1]}・使える枠${m[2]}）`;
  if (e.includes('frozen')) return '終盤のため凍結中の命令が含まれています';
  if (e.includes('more than one manual order')) return '同じ部隊に2つの命令があります';
  if (e.includes('TickFrozen') || e.includes('WrongTick')) return 'このティックは締め切られました。次のティックで出し直してください';
  if (e === 'network') return 'サーバーに届きませんでした';
  return e;
}

// ================================================================== selection
function hoverTile(id) {
  const el = $('#hover-label');
  if (!id || !S.view) { el.hidden = true; return; }
  const t = map.tiles.get(id); const o = ownerOf(id);
  el.hidden = false;
  el.textContent = `${T.TERRAIN[t.terrain]}${t.resource ? '・' + T.RESOURCE[t.resource] : ''}${t.river ? '・川' : ''} · ${t.q}, ${t.r}${o !== null ? ' · ' + civN(o) + 'の領土' : ''}`;
}

function unitsAt(id) { return S.view.units.filter(u => key(u.q, u.r) === id); }
function selectTile(id) {
  if (S.patrol) { // editing a patrol: clicks add waypoints
    const t = map.tiles.get(id);
    if (t && t.terrain !== 'Water' && t.terrain !== 'Mountain' && S.patrol.route.length < 6) { S.patrol.route.push([t.q, t.r]); map.setPatrol(S.patrol); renderInspector(); }
    else if (S.patrol.route.length >= 6) toast('巡回の地点は6つまでです。', 'error');
    else toast('水域や山岳は巡回の地点にできません。', 'error');
    return;
  }
  if (!S.summaryPinned && !S.summaryCollapsed) { S.summaryCollapsed = true; renderSummary(); }
  if (!S.view) return;
  // With one of my units selected, clicking a reachable tile or target acts on it.
  if (S.unit !== null) {
    const u = S.view.units.find(x => x.id === S.unit);
    const mineHere = unitsAt(id).find(x => x.owner === S.me && x.id !== S.unit);
    if (u && !mineHere && id !== key(u.q, u.r)) { S.tile = id; map.setSelection(id); renderInspector(); return; }
  }
  const mine = unitsAt(id).filter(u => u.owner === S.me);
  const myCity = S.view.cities.find(c => key(c.q, c.r) === id && c.owner === S.me);
  const reclick = S.tile === id;
  S.tile = id; map.setSelection(id);
  // Own city: the first click opens the city (production lives there); further clicks cycle its units.
  if (myCity && (!reclick || (S.unit === null && !mine.length))) {
    S.unit = null; S.unitPreview = null; map.setOverlay({}); loadCity(myCity.id); renderInspector(); return;
  }
  if (myCity && S.unit !== null && mine.findIndex(u => u.id === S.unit) === mine.length - 1) {
    S.unit = null; S.unitPreview = null; map.setOverlay({}); loadCity(myCity.id); renderInspector(); return;
  }
  if (mine.length) {
    const cur = mine.findIndex(u => u.id === S.unit);
    selectUnit(mine[(cur + 1) % mine.length].id);
  } else {
    S.unit = null; S.unitPreview = null; map.setOverlay({});
    const c = S.view.cities.find(c => key(c.q, c.r) === id);
    if (c && c.owner === S.me) loadCity(c.id);
    renderInspector();
  }
}
async function selectUnit(id) {
  S.unit = id; S.unitPreview = null; renderInspector();
  const u = S.view.units.find(x => x.id === id);
  if (u) { S.tile = key(u.q, u.r); map.setSelection(S.tile); }
  const p = await api.get(`/api/preview/unit?id=${id}`);
  if (S.unit !== id) return;
  S.unitPreview = p;
  map.setOverlay({
    unit: id,
    reach: new Map(p.reach.map(([q, r, ticks]) => [key(q, r), ticks])),
    attacks: p.attacks.map(a => ({ id: key(a.q, a.r), blocked: a.blocked, forecast: a.forecast })),
    found: p.found !== null && p.found !== undefined ? { id: key(u.q, u.r), ok: !p.found } : null,
  });
  renderInspector();
}
async function loadCity(id) {
  S.cityPreview = null; renderInspector();
  const p = await api.get(`/api/preview/city?id=${id}`);
  S.cityPreview = { id, ...p }; renderInspector();
}
function refreshSelection() {
  if (S.unit !== null) { if (S.view.units.some(u => u.id === S.unit && u.owner === S.me)) selectUnit(S.unit); else { S.unit = null; map.setOverlay({}); } }
  const c = S.tile && S.view.cities.find(c => key(c.q, c.r) === S.tile && c.owner === S.me);
  if (c && S.unit === null) loadCity(c.id);
  if (S.drawer === 'research') loadResearch();
  if (S.drawer === 'diplomacy') loadDiplomacy();
}
async function quickMove(id) {
  if (S.unit === null) return selectTile(id);
  const u = S.view.units.find(x => x.id === S.unit);
  const t = map.tiles.get(id);
  const res = await api.get(`/api/preview/path?unit=${S.unit}&q=${t.q}&r=${t.r}`);
  if (!res.path) return toast(`そこへは移動できません：${moveBlockedText(res.blocked)}`, 'error');
  addDraft({ type: 'MoveUnit', unit: u.id, path: res.path });
}
function moveBlockedText(b) {
  if (b?.code === 'ProtectedCapital') return `${civN(b.civ)}の首都の保護区域です${b.until !== null ? `（ティック${b.until}から入れます）` : '（保護が続く間は入れません）'}`;
  if (b?.code === 'Unreachable') return '途中の道がふさがっているか、12マスより遠い場所です';
  return T.blockedText(b);
}
/** Why the selected unit cannot go to `id` (fetched lazily for the target panel). */
async function loadMoveWhy(unit, id) {
  const t = map.tiles.get(id); const k = `${unit}:${id}:${S.view.tick}`;
  if (S.moveWhy?.key === k) return;
  S.moveWhy = { key: k, text: null };
  const res = await api.get(`/api/preview/path?unit=${unit}&q=${t.q}&r=${t.r}`);
  if (S.moveWhy.key === k) { S.moveWhy.text = res.path ? null : moveBlockedText(res.blocked); renderInspector(); }
}

// ================================================================== render
function renderAll() {
  renderTop(); renderPlate(); renderDock(); renderInspector(); renderNext(); renderSummary(); renderMinimap(); renderNavDots();
  renderRibbon(); renderNextTurn(); renderNotifs(); renderChainBeat(); renderPool(); if (S.chainOpen) renderChainDrawer();
  if (S.drawer) renderDrawer();
}

function renderTop() {
  const v = S.view, e = v.economy, me = civ(S.me);
  const offices = held();
  const badges = S.watch !== undefined ? '<span class="badge">観戦</span>'
    : offices.length ? offices.map(r => `<span class="badge office">${ROLE_GLYPH[r]} ${ROLE_JA[r]}</span>`).join('') : '<span class="badge human">国民</span>';
  setHtml($('#civ-chip'), `<span class="swatch" style="background:${T.CIV_COLORS[S.me]}"></span><div><b>${esc(v.member?.name ?? civN(S.me))}</b><small class="nation">${esc(civN(S.me))}</small><div>${badges}</div></div>`);
  const upkeep = e.upkeepUnits + e.upkeepCities;
  const net = e.goldIncome - upkeep;
  const draftTech = S.drafts.find(d => d.dto.type === 'SetResearch')?.dto.techs[0];
  const res = [
    { g: '◆', c: '#b08a3e', a: fmt(e.gold), n: '金', rate: `${net >= 0 ? '+' : ''}${net} / ティック`, neg: net < 0,
      tip: [['都市の金', `+${e.goldIncome}`], ['部隊の維持費', `−${e.upkeepUnits}`], ['都市の維持費', `−${e.upkeepCities}`], ['差し引き', `${net >= 0 ? '+' : ''}${net}`, true]] },
    { g: '✧', c: '#7b7698', a: e.research ? `${fmt(e.research.store)}<span class="name">/${fmt(e.research.cost)}</span>` : fmt(e.scienceStore), n: '科学',
      rate: e.research ? T.TECH[e.research.tech] : draftTech ? `予定：${T.TECH[draftTech]}（未解決）` : '研究を選んでください', neg: !e.research && !draftTech,
      tip: [['研究中', e.research ? T.TECH[e.research.tech] : 'なし'], ['蓄積', fmt(e.scienceStore)], ['研究済み', `${e.techs.length} / 16`]] },
    { g: '☮', c: '#5b8278', a: fmt(e.influence), n: '影響力', rate: '都市国家への使節に使う', tip: [['用途', '都市国家への使節'], ['宗主になる条件', '影響力60以上で最多']] },
    { g: '⬡', c: '#a27151', a: fmt(e.iron), n: '鉄', rate: `+${e.ironIncome} / ティック`, tip: [['用途', '長槍兵・弩兵（1兵につき1）'], ['産出', '鉄鉱床のある土地を都市が使うと+1']] },
    { g: '♞', c: '#8b6a48', a: fmt(e.horses), n: '馬', rate: `+${e.horseIncome} / ティック`, tip: [['用途', '騎士（1兵につき2）'], ['産出', '馬のいる土地を都市が使うと+1']] },
  ];
  setHtml($('#resources'), res.map(r => `<button class="res" type="button" style="--c:${r.c}" aria-label="${r.n}"><span class="glyph">${r.g}</span><span class="amount">${r.a}<span class="name">${r.n}</span></span><span class="rate ${r.neg ? 'negative' : ''}">${esc(r.rate)}</span>
    <span class="tip panel">${r.tip.map(([k, v, total]) => `<div class="tip-row ${total ? 'total' : ''}"><span>${k}</span><span>${esc(v)}</span></div>`).join('')}</span></button>`).join(''));
  $('#pause-btn').hidden = !!v.chain;
  $('#advance-btn').hidden = true;
  $('#pause-btn').classList.toggle('on', v.paused);
  $('#pause-btn').textContent = v.paused ? '▶' : '❚❚';
  $('#pause-btn').title = v.paused ? '時計を動かす' : '時計を止める（このPCだけの試作）';
  renderClock();
}

function renderClock() {
  const v = S.view; if (!v) return;
  const el = $('#clock');
  let secs = v.secondsLeft;
  if (!v.paused && S.clockBase !== v) { S.clockBase = v; S.clockAt = performance.now(); }
  if (!v.paused) secs = Math.max(0, v.secondsLeft - (performance.now() - (S.clockAt || performance.now())) / 1000);
  const frac = v.tickSeconds ? secs / v.tickSeconds : 1;
  const [, ph, phEn] = T.phaseOf(v.tick);
  el.className = 'clock' + (v.paused ? ' paused' : frac <= .1 ? ' red' : frac <= .25 ? ' amber' : '') + (!v.paused && secs <= 5 ? ' pulse' : '');
  const mm = Math.floor(secs / 60), ss = Math.floor(secs % 60);
  const time = v.over ? '終了' : v.paused ? '停止中' : `${String(mm).padStart(2, '0')}:${String(ss).padStart(2, '0')}`;
  setHtml(el, `<div class="clock-top"><span class="clock-tick">TICK ${String(v.tick).padStart(3, '0')}<span class="clock-of"> / ${v.ticks}</span></span><span class="clock-time">${time}</span></div>
    <div class="clock-phase">${ph} · ${phEn}</div><div class="clock-bar"><i style="width:${(v.paused ? 1 : frac) * 100}%"></i></div>
    <span class="tip panel"><div class="tip-row"><span>1ティック</span><span>${v.tickSeconds}秒</span></div><div class="tip-row"><span>締切で</span><span>全文明を同時に解決</span></div><div class="tip-row"><span>締切3秒前</span><span>下書きを自動確定</span></div></span>`);
  $('#live-dot').classList.toggle('off', !S.online);
}

function renderPlate() {
  const v = S.view, me = civ(S.me);
  const ach = v.achievements?.nations?.[S.me];
  $('#civ-title').textContent = `${civN(S.me)}${ach ? ` · 第${ach.era}時代` : ''}`;
  setHtml($('#civ-sub'), `${me.cities}都市 · 人口${me.pop ?? '?'} · 兵${me.troops ?? '?'} · 国民${(v.members || []).filter(m => m.civ === S.me).length}人${v.economy?.protectionLost ? '' : ' · <span title="他の国はあなたの保護区域に入れません">保護区域あり</span>'}`);
  renderTracker();
}

function renderDock() {
  if (!S.view || !S.view.economy) return;
  const v = S.view, offices = held();
  // Pips per office held; the bank is a reservoir counter, not more pips.
  const pipsOf = role => {
    const o = officeInfo(role), u = used(role);
    let pips = '';
    for (let i = 0; i < o.budget; i++) pips += `<span class="pip ${i < Math.min(u, o.budget) ? 'used' : ''}"></span>`;
    const bankUsed = Math.max(0, u - o.budget);
    const bank = o.bank ? `<span class="bank-well ${bankUsed ? 'drawing' : ''}" title="繰越の枠"><i style="height:${Math.round(100 * (o.bank - bankUsed) / Math.max(1, 4 * o.budget))}%"></i><b>+${o.bank - bankUsed}</b></span>` : '';
    return `<span class="office-pips" title="${ROLE_JA[role]}：このティック ${o.budget}・繰越 ${o.bank}"><span class="og">${ROLE_GLYPH[role]}</span>${pips || '<span class="meta">枠0</span>'}${bank}</span>`;
  };
  const props = S.drafts.filter(d => d.proposal).length;
  setHtml($('#dock-budget'), offices.length
    ? `<div class="pips" aria-label="命令の枠">${offices.map(pipsOf).join('')}</div><div class="budget-text num">${offices.map(r => `${ROLE_JA[r]} ${used(r)}/${spendable(r)}`).join(' · ')}${props ? ` · 献策 ${props}件` : ''}</div>
      <span class="tip panel"><div class="tip-row"><span>国の枠</span><span>3＋都市数（最大8）</span></div><div class="tip-row"><span>役職ごとの割り当て</span><span>将軍・内政官・科学官・外交官</span></div><div class="tip-row"><span>繰越</span><span>役職ごとに最大4ティック分</span></div><div class="tip-row"><span>取引所・同意</span><span>枠を使わない</span></div></span>`
    : `<div class="budget-text">役職がないので、命令はすべて<b>献策</b>になります${props ? `（${props}件）` : ''}</div>`);
  setHtml($('#chips'), S.drafts.length ? S.drafts.map((d, i) => `<span class="chip ${d.usdc ? 'usdc' : ''} ${d.proposal ? 'proposal' : ''}" data-chip="${i}" title="${esc(d.label)}${d.proposal ? `（${ROLE_JA[d.office]}への献策）` : ''}"><span>${d.proposal ? '✎' : d.glyph}</span><span class="t">${esc(d.label)}</span>${d.proposal ? `<span class="pt">献策→${ROLE_JA[d.office]}</span>` : ''}<button class="x" type="button" data-remove="${i}" aria-label="取り消す">×</button></span>`).join('')
    : `<span class="empty">${offices.length ? '命令はまだありません。地図で部隊や都市を選んでください（Space＝次の判断）。担当外の命令は献策になります。' : 'あなたは役職についていません。地図で選んだ命令は、担当の役職者への献策になります。国の広場で投票・支持・リコールもできます。'}</span>`);
  const dirty = isDirty();
  const resolving = !v.paused && v.secondsLeft < .6;
  // Nothing drafted and this tick's turn not ended yet: the button ends the turn (an empty sealed batch per office).
  const idle = !dirty && offices.length && !v.member?.ready;
  const state = v.phase === 'lobby' ? ['draft', '開幕前'] : resolving ? ['resolving', '解決中'] : dirty ? ['draft', `下書き · 未確定`] : idle ? ['draft', '命令なし · 未確定'] : S.drafts.length ? ['committed', '確定済み ✓'] : ['committed', '手番を終えました'];
  const dg = (v.member?.committed || []).find(c => c.digest)?.digest;
  // Commit-reveal as a sealed dispatch: sealed now, opened (and checked) after the tick.
  setHtml($('#digest'), dg && !dirty ? `<span class="seal">🔒 封印 ${dg.slice(0, 6)}…</span>` : dirty && offices.length ? '<span class="seal">確定で封印</span>' : '');
  $('#digest').title = dg ? `decision_digest ${dg}\nobs_root ${v.decision?.obsRoot}` : '';
  setHtml($('#commit'), `<span class="commit-state ${state[0]}">${state[1]}</span><button class="btn ${dirty ? 'primary' : ''}" type="button" id="commit-btn" ${S.committing || v.over || v.phase === 'lobby' || S.watch !== undefined || (!dirty && !idle) ? 'disabled' : ''}>${dirty ? (offices.length ? '確定する' : '献策する') : idle ? '命令なしで手番を終える' : '確定済み'}<span class="key" style="margin-left:4px">⌃↵</span></button>`);
}

// ================================================================== inspector
function renderInspector() {
  const el = $('#inspector');
  if (!S.view) return;
  el.classList.toggle('idle', S.unit === null && !S.tile && !S.patrol); // phones hide the idle intro to keep the map visible
  if (S.patrol) return setHtml(el, patrolPanel());
  if (S.unit !== null) {
    const u = S.view.units.find(x => x.id === S.unit);
    if (u && S.tile && S.tile !== key(u.q, u.r)) return setHtml(el, targetPanel(u, S.tile));
    if (u) return setHtml(el, unitPanel(u));
  }
  if (!S.tile) return setHtml(el, introPanel());
  const c = S.view.cities.find(c => key(c.q, c.r) === S.tile);
  if (c) return setHtml(el, c.owner === S.me ? myCityPanel(c) : foreignCityPanel(c));
  const cs = S.view.cityStates.find(c => key(c.q, c.r) === S.tile && c.capturedBy === null);
  if (cs) return setHtml(el, cityStatePanel(cs));
  const others = unitsAt(S.tile);
  setHtml(el, tilePanel(map.tiles.get(S.tile), others));
}
const head = (eyebrow, title, desc = '') => `<button class="close-x" type="button" data-close aria-label="閉じる">×</button><div class="eyebrow">${eyebrow}</div><h2>${title}</h2>${desc ? `<p class="desc">${desc}</p>` : ''}`;

function introPanel() {
  const cap = S.view.cities.find(c => c.id === civ(S.me).capital);
  const mine = held();
  return `${head('YOUR TURN · このティック', mine.length ? `あなたは${mine.map(r => ROLE_JA[r]).join('・')}` : 'あなたは国民（役職なし）', '都市・部隊・土地をクリックすると、ここに詳細と「できること」が出ます。できないことには理由が表示されます。')}
    <div class="explanation">${esc(othersText(S.view))}${mine.length ? '担当の役職の命令は封印して送り、担当外の命令は献策になります。' : 'あなたの命令は担当の役職者への献策になります。採用されると功績を半分ずつ分けます。'}</div>
    <button class="btn wide" type="button" data-drawer="nation">⚖ 国の広場を開く（選挙・献策・リコール）</button>
    ${cap ? `<button class="btn primary wide" type="button" data-focus="${key(cap.q, cap.r)}">⌖ 首都 ${T.cityName(cap.id)} を見る</button>` : ''}`;
}

const fogOf = id => S.view?.fog?.[tileIndex(id)] ?? '2';
const FOG_NOTE = {
  0: '<div class="explanation fog">未踏の地です。地形は公開の世界シードから分かりますが、部隊・都市・国境は見えません。偵察で視界に入れてください。</div>',
  1: '<div class="explanation fog">霧の中です。国境と都市は最後に見たときのまま表示しています。部隊は視界の中でしか見えません。</div>',
};
function tilePanel(t, units) {
  const o = ownerOf(t.id); const fog = fogOf(t.id);
  const y = { Grassland: [2, 0, 0], Plains: [1, 1, 0], Forest: [1, 2, 0], Hills: [0, 2, 0], Mountain: [0, 0, 0], Water: [1, 0, 1] }[t.terrain].slice();
  if (t.river) y[2]++; if (t.resource === 'Wheat') y[0] += 2; if (t.resource === 'Iron') y[1]++; if (t.resource === 'Horses') y[0]++;
  const hub = S.map.hubs.some(([q, r]) => q === t.q && r === t.r);
  return `${head(`${T.TERRAIN_EN[t.terrain]} · ${t.q}, ${t.r}`, T.TERRAIN[t.terrain], fog === '0' ? '誰の領土かはまだ分かりません。' : o !== null ? `${esc(civN(o))}の領土${fog === '1' ? '（最後に見たとき）' : ''}です。` : 'どの文明の領土でもありません。')}
    ${FOG_NOTE[fog] || ''}
    <div class="tags">${t.resource ? `<span class="tag positive">${T.RESOURCE[t.resource]}</span>` : ''}${t.river ? '<span class="tag">川 · 金+1</span>' : ''}${hub ? '<span class="tag warning">交易拠点 · 金の市場の手数料1%を得る</span>' : ''}${t.terrain === 'Mountain' || t.terrain === 'Water' ? '<span class="tag bad">通行不可</span>' : `<span class="tag">移動コスト ${t.terrain === 'Forest' || t.terrain === 'Hills' ? 2 : 1}</span>`}${t.terrain === 'Forest' || t.terrain === 'Hills' ? '<span class="tag">守備側の被害 −20%</span>' : ''}</div>
    <div class="stats"><div class="stat"><div class="k">食料</div><div class="v">${y[0]}</div></div><div class="stat"><div class="k">生産</div><div class="v">${y[1]}</div></div><div class="stat"><div class="k">金</div><div class="v">${y[2]}</div></div></div>
    ${units.length ? `<div class="section-title">この土地の部隊</div>${units.map(u => `<div class="list-row"><div class="main"><div class="title"><span class="swatch-s" style="background:${u.owner === 'barbarian' ? '#5f5a52' : T.CIV_COLORS[u.owner]}"></span>${esc(u.owner === 'barbarian' ? '蛮族' : civN(u.owner))}の${T.UNIT[u.type]}</div><div class="meta">${u.civilian ? '非戦闘' : `兵 ${(u.troops / 10).toFixed(1)}`}</div></div></div>`).join('')}` : ''}
    <p class="desc">土地は都市の領土に入ると、その都市の人口に応じて自動で使われます（産出の高い順）。</p>`;
}

function unitPanel(u) {
  const p = S.unitPreview;
  const draft = S.drafts.find(d => slotOf(d.dto) === `u${u.id}`);
  const idle = !u.path?.length && !draft && !u.standing;
  let html = head(`${u.civilian ? 'CIVILIAN' : 'ARMY'} · ${u.q}, ${u.r}`, `${T.UNIT[u.type]}${u.civilian ? '' : ` <span style="font-size:15px;color:var(--muted)">兵 ${(u.troops / 10).toFixed(1)}</span>`}`,
    u.type === 'Settler' ? '都市を建てられる場所（都市から3マス以上、他国の領土・保護区域の外）へ移動して建設します。' : u.type === 'Scout' ? '森や丘陵でも1ずつ進める偵察役です。戦闘はできません。' : '移動先の土地をクリックかダブルクリック。攻撃できる相手は赤い枠で示されます。');
  html += `<div class="tags">${idle ? '<span class="tag warning">待機中</span>' : u.path?.length ? `<span class="tag">移動中 · 残り${u.path.length}マス</span>` : ''}${draft ? `<span class="tag positive">命令あり：${esc(draft.label)}</span>` : ''}</div>`;
  if (!p) return html + '<p class="desc">行動を計算しています…</p>';
  if (u.type === 'Settler') {
    html += p.found ? `<button class="btn wide" type="button" aria-disabled="true" disabled>⌂ ここに都市を建てる</button><p class="option why" style="margin:4px 0">${esc(T.blockedText(p.found))}</p>`
      : `<button class="btn primary wide" type="button" data-order='${JSON.stringify({ type: 'FoundCity', settler: u.id })}'>⌂ ここに都市を建てる（枠1）</button><p class="desc">建設すると開拓者は消え、半径1の土地が領土になります。</p>`;
  }
  if (!u.civilian) {
    const targets = p.attacks;
    html += `<div class="section-title">攻撃できる相手</div>`;
    html += targets.length ? targets.map(a => attackOption(u, a)).join('') : '<p class="desc">射程内に相手はいません。戦争中の相手・蛮族・都市国家を攻撃できます（弓兵・弩兵は2マス、ほかは隣接）。</p>';
  }
  html += standingSection(u);
  const now1 = p.reach.filter(r => r[2] <= 1).length, soon = p.reach.filter(r => r[2] > 1 && r[2] <= 3).length;
  html += `<div class="section-title">移動</div><div class="legend-row"><span><span class="sw sw-now"></span>このティック ${now1}マス</span><span><span class="sw sw-soon"></span>2〜3ティック ${soon}マス</span>${S.view.protectionRadius ? `<span><span class="sw sw-prot"></span>他国首都の保護区域（半径${S.view.protectionRadius}）</span>` : ''}</div>
    <p class="desc">行き先をダブルクリック（または選んで Enter）で下書きに入ります。遠い土地はカーソルを当てると到着ティックが出ます。移動は締切で全員同時に解決されます。</p>`;
  return html;
}
/** Standing rules for one of my units (§13): set once for 1 slot, then free every tick. */
function standingSection(u) {
  const cur = u.standing; const draft = S.drafts.find(d => d.dto.type === 'SetStanding' && d.dto.target.kind === 'Unit' && d.dto.target.id === u.id);
  const order = rule => esc(JSON.stringify({ type: 'SetStanding', target: { kind: 'Unit', id: u.id }, rule }));
  const btn = (rule, text, on) => `<button class="btn ${on ? 'primary' : ''}" type="button" data-order='${order(rule)}' ${on ? 'disabled' : ''}>${text}</button>`;
  let html = `<div class="section-title">継続命令 · 毎ティック自動（設定に枠1・実行は無料）</div>`;
  html += `<div class="standing-now">${cur ? `<span class="tag positive">${T.STANDING_GLYPH[cur.kind]} ${esc(T.standingText(cur))}</span>` : '<span class="tag">なし</span>'}${draft ? `<span class="tag warning">下書き：${esc(T.standingText(draft.dto.rule))}</span>` : ''}</div>`;
  if (!u.civilian) {
    const eff = draft ? (draft.dto.rule.kind === 'Clear' ? null : draft.dto.rule) : cur; // show what will be in force
    const r = eff?.kind === 'AutoDefend' ? eff.radius : 0;
    html += `<div class="standing-row"><span class="sl">${T.STANDING_GLYPH.AutoDefend} 自動防衛</span>${[1, 2, 3].map(n => btn({ kind: 'AutoDefend', radius: n }, `半径${n}`, r === n)).join('')}</div>`;
    const q = eff?.kind === 'Retreat' ? eff.ratioBps : 0;
    html += `<div class="standing-row"><span class="sl">${T.STANDING_GLYPH.Retreat} 撤退</span>${[[10000, '1倍'], [15000, '1.5倍'], [20000, '2倍']].map(([b, t]) => btn({ kind: 'Retreat', ratioBps: b }, t, q === b)).join('')}</div>`;
  }
  html += `<div class="standing-row"><span class="sl">${T.STANDING_GLYPH.Patrol} 巡回</span><button class="btn" type="button" id="patrol-start" data-unit="${u.id}">${cur?.kind === 'Patrol' ? '道筋を引き直す' : '地図で地点を選ぶ'}</button>${cur ? btn({ kind: 'Clear' }, '解除', false) : ''}</div>`;
  html += `<p class="desc">${u.civilian ? '' : '自動防衛：基点から半径内に入った敵軍のうち最も弱いものを攻撃。撤退：隣の敵の強さが自軍の指定倍を超えたら自分の都市へ1マス下がる。'}巡回：最大6地点を順に回り続けます。手動の命令を出したティックはそちらが優先されます。</p>`;
  return html;
}
function citySection(c) {
  const st = c.standing || { repeatQueue: true, autoPurchase: 0 };
  const order = rule => esc(JSON.stringify({ type: 'SetStanding', target: { kind: 'City', id: c.id }, rule }));
  const pending = kind => S.drafts.find(d => d.dto.type === 'SetStanding' && d.dto.target.kind === 'City' && d.dto.target.id === c.id && d.dto.rule.kind === kind);
  const rp = pending('QueueRepeat'), ap = pending('AutoPurchase');
  const repeat = rp ? rp.dto.rule.on : st.repeatQueue, buy = ap ? ap.dto.rule.maxGold : st.autoPurchase;
  return `<div class="section-title">継続命令 · 毎ティック自動${rp || ap ? ' · 下書きあり' : ''}</div>
    <div class="standing-row"><span class="sl">${T.STANDING_GLYPH.QueueRepeat} 生産の繰り返し</span>${[[true, 'ON'], [false, 'OFF']].map(([on, t]) => `<button class="btn ${repeat === on ? 'primary' : ''}" type="button" data-order='${order({ kind: 'QueueRepeat', on })}' ${repeat === on ? 'disabled' : ''}>${t}</button>`).join('')}</div>
    <div class="standing-row"><span class="sl">${T.STANDING_GLYPH.AutoPurchase} 自動購入</span>${[0, 20, 40, 80].map(g => `<button class="btn ${buy === g ? 'primary' : ''}" type="button" data-order='${order({ kind: 'AutoPurchase', maxGold: g })}' ${buy === g ? 'disabled' : ''}>${g ? `${g}金` : 'なし'}</button>`).join('')}</div>
    <p class="desc">繰り返し：予定が空になったら最後の部隊をもう一度作ります。自動購入：毎ティック指定額まで金で生産を進めます（手動の購入をしたティックは除く）。</p>`;
}
function patrolPanel() {
  const pt = S.patrol; const u = S.view.units.find(x => x.id === pt.unit);
  let html = head(`PATROL · ${T.UNIT[u?.type] || ''}`, '巡回の道筋', '地図で回る地点を順にクリックしてください（最大6）。最後の地点のあとは最初に戻ります。');
  html += `<ol class="patrol-list">${pt.route.map(([q, r], i) => `<li>${i + 1}. (${q}, ${r}) <button class="close-x" type="button" data-patrol-remove="${i}" aria-label="削除">×</button></li>`).join('') || '<li class="meta">まだ地点がありません</li>'}</ol>`;
  const dto = { type: 'SetStanding', target: { kind: 'Unit', id: pt.unit }, rule: { kind: 'Patrol', route: pt.route } };
  html += `<div class="row"><button class="btn primary" type="button" id="patrol-done" ${pt.route.length ? '' : 'disabled'} data-dto='${esc(JSON.stringify(dto))}'>この道筋で巡回（枠1）</button><button class="btn" type="button" id="patrol-cancel">やめる</button></div>`;
  return html;
}

function attackOption(u, a) {
  const t = a.target;
  let name = '';
  if (t.kind === 'Unit') { const y = S.view.units.find(x => x.id === t.id); name = `${civN(y?.owner)}の${T.UNIT[y?.type]}`; }
  if (t.kind === 'City') name = `${T.cityName(t.id)}（都市）`;
  if (t.kind === 'CityState') name = `都市国家${t.id + 1}`;
  const dto = { type: 'Attack', army: u.id, target: t };
  if (a.blocked) return `<div class="option" aria-disabled="true"><span class="ic">⚔</span><span><span class="name">${esc(name)}</span><div class="why">${esc(T.blockedText(a.blocked))}</div></span><span></span></div>`;
  const f = a.forecast;
  const detail = f.captureCivilian ? '非戦闘ユニットを捕獲します' : `予測：相手 −${(f.toDefender / 1000).toFixed(1)}（残り${((f.defenderTroops - f.toDefender) / 1000).toFixed(1)}）・自軍 −${(f.toAttacker / 1000).toFixed(1)}`;
  const warn = f.neutral ? '<div class="why">中立への攻撃は侵略扱い：都市国家への影響力をすべて失います</div>' : '';
  return `<button class="option" type="button" data-order='${esc(JSON.stringify(dto))}'><span class="ic">⚔</span><span><span class="name">${esc(name)}</span><div class="meta">${detail}</div>${warn}<div class="meta">乱数±10%・他の文明も同時に動くため目安です</div></span><span class="cost">枠1</span></button>`;
}

function targetPanel(u, id) {
  const p = S.unitPreview; const t = map.tiles.get(id);
  const reach = p?.reach.find(([q, r]) => q === t.q && r === t.r);
  const atk = p?.attacks.filter(a => key(a.q, a.r) === id) || [];
  let html = head(`${T.UNIT[u.type]} → ${t.q}, ${t.r}`, T.TERRAIN[t.terrain], '選択中の部隊で、この土地に対してできること。');
  html += atk.map(a => attackOption(u, a)).join('');
  const di = S.drafts.findIndex(d => d.dto.type === 'MoveUnit' && d.dto.unit === u.id);
  const here = di >= 0 && S.drafts[di].focus === id;
  if (reach && here) html += `<div class="explanation ok">✓ この土地への移動は下書き済みです（約${reach[2]}ティック）。</div><button class="btn wide" type="button" data-remove="${di}">下書きを取り消す</button>`;
  else if (reach) html += `<button class="btn primary wide" type="button" data-move="${id}">→ ここへ移動（約${reach[2]}ティック・${di >= 0 ? '今の移動命令と差し替え' : '枠1'}）</button>`;
  else if (!atk.length) {
    loadMoveWhy(u.id, id);
    const why = S.moveWhy?.key === `${u.id}:${id}:${S.view.tick}` ? S.moveWhy.text : null;
    html += `<div class="explanation">この部隊はここへ移動できません${why ? `：${esc(why)}` : '。'}</div>`;
  }
  html += `<button class="btn wide" type="button" data-select-unit="${u.id}" style="margin-top:6px">← ${T.UNIT[u.type]}に戻る</button>`;
  return html;
}

function myCityPanel(c) {
  const p = S.cityPreview?.id === c.id ? S.cityPreview : null;
  const o = p?.outlook;
  const qDraft = S.drafts.find(d => d.dto.type === 'SetQueue' && d.dto.city === c.id);
  const queue = qDraft ? qDraft.dto.items : (c.queue || []);
  const fDraft = S.drafts.find(d => d.dto.type === 'SetFocus' && d.dto.city === c.id);
  const focus = fDraft ? fDraft.dto.focus : c.focus;
  let html = head(`${c.capital ? 'CAPITAL · 首都' : 'CITY · 都市'} · ${c.q}, ${c.r}`, T.cityName(c.id), '');
  html += `<div class="stats">
    <div class="stat"><div class="k">人口</div><div class="v">${c.pop}</div></div>
    <div class="stat"><div class="k">食料</div><div class="v">${o ? (o.foodSurplus >= 0 ? '+' : '') + o.foodSurplus : '…'}<small>/t</small></div></div>
    <div class="stat"><div class="k">生産</div><div class="v">${o ? o.production : '…'}<small>/t</small></div></div>
    <div class="stat"><div class="k">防御</div><div class="v">${(c.defense / 10).toFixed(1)}<small>/${(c.defenseMax / 10).toFixed(0)}</small></div></div>
    <div class="stat"><div class="k">忠誠</div><div class="v">${c.loyalty}</div></div>
    <div class="stat"><div class="k">快適度</div><div class="v">${o ? o.amenities : '…'}</div></div></div>`;
  if (o) html += `<div class="meter"><span>成長：${c.food} / ${o.growthThreshold}${o.ticksToGrow ? `（あと約${o.ticksToGrow}ティック）` : '（食料が足りず停滞）'}</span><div class="bar"><i style="width:${Math.min(100, c.food / o.growthThreshold * 100)}%"></i></div></div>`;
  html += `<div class="tags">${(c.buildings || []).map(b => `<span class="tag">${T.BUILDING_GLYPH[b]} ${T.BUILDING[b]}</span>`).join('') || '<span class="tag">建物なし</span>'}</div>`;
  const here = unitsAt(key(c.q, c.r)).filter(u => u.owner === S.me);
  if (here.length) html += `<div class="section-title">この都市の部隊（もう一度クリックでも選べます）</div><div class="row">${here.map(u => `<button class="btn" type="button" data-select-unit="${u.id}">${T.UNIT_GLYPH[u.type]} ${T.UNIT[u.type]}${u.civilian ? '' : ` 兵${(u.troops / 10).toFixed(1)}`}</button>`).join('')}</div>`;
  html += `<div class="section-title">方針（土地の割り当て）${fDraft ? ' · 命令あり' : ''}</div><div class="row">${Object.entries(T.FOCUS).map(([k, n]) => `<button class="btn ${k === focus ? 'primary' : ''}" type="button" data-order='${JSON.stringify({ type: 'SetFocus', city: c.id, focus: k })}' ${k === focus ? 'disabled' : ''}>${n}</button>`).join('')}</div>`;
  html += citySection(c);
  html += `<div class="section-title">生産予定 ${qDraft ? '· 命令あり（確定で置き換わります）' : ''}</div>`;
  html += queue.length ? queue.map((it, i) => `<div class="option queued"><span class="ic">${T.itemGlyph(it)}</span><span><span class="name">${i + 1}. ${esc(T.itemName(it))}</span>${i === 0 ? `<div class="meta">蓄積 ${c.prod}${o ? ` · 毎ティック +${o.production}` : ''}</div>` : ''}</span><span></span></div>`).join('')
    + `<div class="row"><button class="btn" type="button" data-queue-clear="${c.id}">予定をクリア（枠1）</button>${queue.length ? `<button class="btn" type="button" data-order='${JSON.stringify({ type: 'Purchase', city: c.id, gold: 60 })}'>◆ 60金で生産を購入</button>` : ''}</div>`
    : `<div class="explanation">生産予定がありません。生産は都市に蓄積されるので無駄にはなりませんが、下から選んでください。</div>`;
  html += `<div class="section-title">追加できるもの（最大3件・枠1）</div>`;
  if (!p) return html + '<p class="desc">選択肢を計算しています…</p>';
  html += p.options.map(opt => {
    const it = opt.item; const blocked = opt.blocked;
    const effect = it.kind === 'Building' ? T.BUILDING_EFFECT[it.building] : it.kind === 'Troops' ? `${T.UNIT[it.unit]}の部隊（兵5）` : it.kind === 'Settler' ? '新しい都市を建てる（人口−1）' : '偵察用';
    const next = [...queue, it].slice(0, 3);
    const dto = { type: 'SetQueue', city: c.id, items: next };
    const full = queue.length >= 3;
    return `<button class="option" type="button" ${blocked || full ? 'aria-disabled="true"' : `data-order='${esc(JSON.stringify(dto))}'`}><span class="ic">${T.itemGlyph(it)}</span><span><span class="name">${esc(T.itemName(it))}</span><div class="meta">${esc(effect)}${opt.ticks ? ` · 約${opt.ticks}ティック` : ''}</div>${blocked ? `<div class="why">${esc(T.blockedText(blocked))}</div>` : full ? '<div class="why">予定は3件までです</div>' : ''}</span><span class="cost">${opt.cost}<small style="font-size:9px;color:var(--muted)"> 生産</small></span></button>`;
  }).join('');
  return html;
}

function foreignCityPanel(c) {
  const o = c.owner; const rel = o === null ? null : civ(o)?.relation;
  const mine = S.view.units.filter(u => u.owner === S.me && !u.civilian && hexDist(u, c) <= 2);
  let html = head(`${o === null ? 'FREE CITY · 自由都市' : 'CITY · 都市'} · ${c.q}, ${c.r}`, T.cityName(c.id), o === null ? 'どの文明にも属さない自由都市です。攻撃は侵略扱いになります。' : `${esc(civN(o))}の${c.capital ? '首都' : '都市'}です。関係：${T.RELATION[rel]}`);
  if (c.seenTick !== null && c.seenTick !== undefined) html += `<div class="explanation fog">ティック${c.seenTick}に見たときの情報です。今の持ち主・人口・防御は視界に入れるまで分かりません。</div>`;
  html += `<div class="stats"><div class="stat"><div class="k">人口</div><div class="v">${c.pop}</div></div><div class="stat"><div class="k">防御</div><div class="v">${(c.defense / 10).toFixed(1)}<small>/${(c.defenseMax / 10).toFixed(0)}</small></div></div><div class="stat"><div class="k">城壁</div><div class="v">${c.walls ? 'あり' : 'なし'}</div></div></div>`;
  if (c.stages) html += `<div class="explanation">スターゲート ${c.stages}/3 段階。この都市を占領すると、完成した段階はすべて失われます。</div>`;
  html += mine.length ? `<div class="section-title">近くのあなたの部隊</div>${mine.map(u => `<button class="option" type="button" data-select-unit="${u.id}"><span class="ic">${T.UNIT_GLYPH[u.type]}</span><span><span class="name">${T.UNIT[u.type]} 兵${(u.troops / 10).toFixed(1)}</span><div class="meta">選択して攻撃の予測を見る</div></span><span></span></button>`).join('')}` : '';
  if (rel && rel !== 'war' && o !== null) html += `<button class="btn wide" type="button" data-drawer="diplomacy">✉ ${esc(civN(o))}との外交を開く</button>`;
  return html;
}

function cityStatePanel(cs) {
  const me = S.view.economy.influence;
  let html = head(`CITY-STATE · 都市国家 · ${cs.q}, ${cs.r}`, `都市国家 ${cs.id + 1}`, `${T.SPECIALTY[cs.specialty]}の都市国家。${T.SPECIALTY_BONUS[cs.specialty]}。`);
  html += `<div class="stats"><div class="stat"><div class="k">宗主</div><div class="v" style="font-size:14px">${cs.suzerain === null ? 'なし' : esc(civN(cs.suzerain))}</div></div><div class="stat"><div class="k">あなたの影響力</div><div class="v">${cs.myInfluence}</div></div><div class="stat"><div class="k">最多</div><div class="v">${cs.topInfluence}</div></div></div>`;
  html += `<div class="meter"><span>宗主になる条件：影響力60以上で最多（45ティックごとに見直し、全員の影響力が半分に）</span><div class="bar gold"><i style="width:${Math.min(100, cs.myInfluence / 60 * 100)}%"></i></div></div>`;
  html += `<div class="section-title">使節を送る（枠1・所持影響力 ${me}）</div><div class="row">${[10, 20, 40].map(n => `<button class="btn" type="button" ${me < n ? 'disabled title="影響力が足りません"' : ''} data-order='${JSON.stringify({ type: 'SendEnvoy', cityState: cs.id, influence: n })}'>影響力 ${n}</button>`).join('')}</div>`;
  html += `<div class="explanation">都市国家への攻撃は侵略扱いです：すべての都市国家への影響力を失います（協調の道の節目に響きます）。</div>`;
  return html;
}

/** Send one governance action (vote, support, stand, recall). */
async function govAction(action, el) {
  if (el) el.disabled = true;
  const r = await api.post('/api/gov', { action }).catch(() => ({ ok: false, error: 'network' }));
  const what = { Vote: '投票', Support: '支持', Stand: '立候補', Recall: 'リコールに賛成', Propose: '献策' }[action.type];
  if (r.ok) {
    if (action.type === 'Vote') { S.myVotes ??= {}; S.myVotes[action.role] = action.candidate; }
    toast(`${what}しました。${S.view.chain ? 'チェーンに記録しました' : '次のティックで反映されます'}。`);
  } else toast(`${what}できませんでした：${translateError(r.error)}`, 'error');
  poll();
}

// ================================================================== next decision
function nextItems() {
  const v = S.view; const items = [];
  if (!v.economy) return [{ pri: 0, title: '観戦中', detail: '', none: true }];
  const mine = held(), g = v.gov || {};
  if (g.voteOpen && ROLES.some(r => g.candidates?.find(c => c.role === r)?.candidates.length && S.myVotes?.[r] === undefined)) items.push({ pri: 95, title: `選挙の投票受付中（ティック${g.nextElection}の任期）`, detail: '国の広場で各役職に投票できます。', drawer: 'nation' });
  for (const r of g.recalls || []) items.push({ pri: 92, title: `${ROLE_JA[r.role]}のリコール投票`, detail: `賛成${r.yes}件 · ティック${r.closes}まで`, drawer: 'nation' });
  const adoptable = (g.proposals || []).filter(p => mine.includes(p.role) && p.proposer?.id !== S.member && !(S.adopt?.[p.role] || []).includes(p.id));
  if (adoptable.length) items.push({ pri: 88, title: `あなた宛ての献策 ${adoptable.length}件`, detail: '採用すると功績を献策者と半分ずつ分けます。', drawer: 'nation' });
  const draftUnits = new Set(S.drafts.map(d => d.dto.unit ?? d.dto.army ?? d.dto.settler).filter(x => x !== undefined));
  for (const p of v.proposals.filter(p => p.to === S.me && mine.includes('Diplomat'))) items.push({ pri: 100, title: `${civN(p.from)}の${({ Peace: '講和', Nap: '不可侵条約', Alliance: '同盟' })[p.kind]}の申し入れ`, detail: `ティック${p.expires}まで有効。外交パネルで受けるか決めてください。`, drawer: 'diplomacy' });
  for (const u of myUnits()) {
    if (u.path?.length || draftUnits.has(u.id)) continue;
    if (!mine.includes(u.type === 'Settler' ? 'Steward' : 'General')) continue;
    if (u.standing && u.type !== 'Settler') continue; // a standing rule is looking after it
    if (u.type === 'Settler') items.push({ pri: 90, title: '開拓者が待機中', detail: '都市を建てられる土地へ移動するか、この場で建設します。', unit: u.id });
    else if (!u.civilian && Object.values(v.civs).some(c => c.relation === 'war') ) items.push({ pri: 70, title: `${T.UNIT[u.type]}が待機中（戦争中）`, detail: '攻撃できる相手がいないか確認するか、前線へ移動させます。', unit: u.id });
  }
  for (const c of mine.includes('Steward') ? myCities() : []) {
    const qd = S.drafts.some(d => d.dto.type === 'SetQueue' && d.dto.city === c.id);
    if (!qd && !(c.queue || []).length) items.push({ pri: 80, title: `${T.cityName(c.id)}の生産予定が空です`, detail: '建物・部隊・開拓者から選びます。生産は蓄積されるので無駄にはなりません。', tile: key(c.q, c.r) });
  }
  if (mine.includes('Science') && !v.economy.researchQueue.length && !S.drafts.some(d => d.dto.type === 'SetResearch')) items.push({ pri: 85, title: '研究が選ばれていません', detail: '科学は蓄積されますが、研究を選ぶと技術が解放されます。', drawer: 'research' });
  const left = spendable() - used();
  if (mine.length && left > 0 && !v.paused && v.secondsLeft < v.tickSeconds * .25) items.push({ pri: 60, title: `命令の枠が${left}残っています`, detail: '使わない枠は役職ごとに繰り越されます（最大4ティック分）。', none: true });
  if (!items.length) items.push({ pri: 0, title: '急ぎの判断はありません', detail: mine.length ? '地図を眺めて、拡大・研究・外交の次の一手を考えましょう。' : '役職外の命令は献策として出せます。国の広場で他の国民の献策を支持しましょう。', none: true });
  return items.sort((a, b) => b.pri - a.pri);
}
function renderNext() {
  if (!S.view) return;
  const items = nextItems(); S.nextIndex %= items.length; const it = items[S.nextIndex];
  setHtml($('#next-action'), `<div class="next-head"><span class="eyebrow">次の判断 · NEXT DECISION</span><button class="icon-btn" type="button" id="next-skip" title="次へ（Space）" style="width:26px;height:26px">↻</button></div>
    <div class="row" style="justify-content:space-between;flex-wrap:nowrap;align-items:flex-start"><div><div class="next-title">${esc(it.title)}</div><div class="next-detail">${esc(it.detail)}</div></div>${it.none ? '' : '<button class="round-btn" type="button" id="next-go" title="そこへ移動">↗</button>'}</div>
    <div class="next-foot"><span>${S.nextIndex + 1} / ${items.length} 件 · 提案は自由に選べます</span><span><span class="key">Space</span></span></div>`);
  S.nextCurrent = it;
}
function goNext(advance = true) {
  const it = S.nextCurrent; if (!it) return;
  if (it.unit !== undefined) selectUnit(it.unit), map.focusTile(key(S.view.units.find(u => u.id === it.unit).q, S.view.units.find(u => u.id === it.unit).r));
  else if (it.tile) { selectTile(it.tile); map.focusTile(it.tile); }
  else if (it.drawer) openDrawer(it.drawer);
  if (advance) S.nextIndex++;
}

// ================================================================== summary
/** What changed for *me* this tick, diffed client-side from the previous view. */
function digest(a, b) {
  const me = b.me, out = [];
  const ea = a.economy, eb = b.economy;
  const dg = eb.gold - ea.gold;
  if (dg) out.push(['◆', `金 ${dg > 0 ? '+' : ''}${dg}（${eb.gold}）`]);
  if (ea.research && !eb.techs.includes(ea.research.tech) && eb.research?.tech === ea.research.tech) out.push(['✧', `研究「${T.TECH[eb.research.tech]}」${eb.research.store}/${eb.research.cost}`]);
  else if (eb.research && !ea.research) out.push(['✧', `研究「${T.TECH[eb.research.tech]}」を開始（${eb.research.store}/${eb.research.cost}）`]);
  for (const t of eb.techs.filter(t => !ea.techs.includes(t))) out.push(['✦', `「${T.TECH[t]}」を発見：${T.TECH_UNLOCK[t]}`]);
  const ca = new Map(a.cities.filter(c => c.owner === me).map(c => [c.id, c]));
  for (const c of b.cities.filter(c => c.owner === me)) {
    const o = ca.get(c.id);
    if (!o) { out.push(['⌂', `${T.cityName(c.id)}があなたの都市に`]); continue; }
    if (c.pop > o.pop) out.push(['❧', `${T.cityName(c.id)}の人口が${c.pop}に`]);
    for (const bld of c.buildings.filter(x => !o.buildings.includes(x))) out.push(['▤', `${T.cityName(c.id)}で${T.BUILDING[bld] || bld}が完成`]);
    if (c.defense < o.defense) out.push(['!', `${T.cityName(c.id)}が攻撃を受けた（防御 ${(c.defense / 10).toFixed(0)}/${(c.defenseMax / 10).toFixed(0)}）`]);
  }
  for (const id of ca.keys()) if (!b.cities.some(c => c.id === id && c.owner === me)) out.push(['!', `${T.cityName(id)}を失った`]);
  const ua = new Map(a.units.filter(u => u.owner === me).map(u => [u.id, u]));
  for (const u of b.units.filter(u => u.owner === me)) {
    const o = ua.get(u.id);
    if (!o) { out.push(['＋', `${T.UNIT[u.type]}が誕生`]); continue; }
    if (o.q !== u.q || o.r !== u.r) out.push(['→', `${T.UNIT[u.type]}が移動（${u.q}, ${u.r}）${u.path?.length ? `・残り${u.path.length}マス` : ''}`, key(u.q, u.r)]);
    if (u.troops < o.troops) out.push(['⚔', `${T.UNIT[u.type]}が損害 −${((o.troops - u.troops) / 10).toFixed(1)}（残り${(u.troops / 10).toFixed(1)}）`, key(u.q, u.r)]);
  }
  for (const id of ua.keys()) if (!b.units.some(u => u.id === id && u.owner === me)) out.push(['✕', `${T.UNIT[ua.get(id).type]}を失った`]);
  return out;
}

function renderSummary() {
  const v = S.view, el = $('#summary');
  if (v.resolvedTick === null || !S.summaryOpen) { el.hidden = true; return; }
  el.hidden = false;
  const lines = (v.lastSummary || []).map(T.chronicleText);
  const mine = S.digest || [];
  // Once the player starts working the map, the report folds to one line (still one click away).
  const collapsed = !!S.summaryCollapsed && (S.tile !== null || S.unit !== null);
  el.classList.toggle('collapsed', collapsed);
  const count = mine.length + lines.length;
  setHtml(el, `<div class="summary-head"><button class="summary-toggle" type="button" id="summary-toggle" aria-expanded="${!collapsed}"><span class="eyebrow">ティック ${v.resolvedTick} の結果${collapsed ? ` · ${count}件` : ''}</span><span class="chev">${collapsed ? '▾' : '▴'}</span></button><button class="close-x" type="button" id="summary-close" aria-label="閉じる">×</button></div>
    ${mine.length ? `<div class="section-title">あなたの文明</div><ul>${mine.map(([g, t, at]) => `<li ${at ? `class="go" data-focus="${at}" title="地図で見る"` : ''}><span>${g}</span><span>${esc(t)}</span></li>`).join('')}</ul>` : ''}
    <div class="section-title">世界</div>
    ${lines.length ? `<ul>${lines.map(([k, t]) => `<li><span>${T.KIND_GLYPH[k] || '·'}</span><span>${esc(t)}</span></li>`).join('')}</ul>` : '<p class="desc" style="margin:4px 0 0">世界に大きな出来事はありませんでした。</p>'}`);
}

// ================================================================== drawers
function openDrawer(name) {
  S.drawer = S.drawer === name ? null : name;
  document.querySelectorAll('.nav-btn').forEach(b => b.classList.toggle('active', b.dataset.drawer === S.drawer));
  $('#drawer').hidden = !S.drawer;
  if (S.drawer === 'research') loadResearch();
  if (S.drawer === 'diplomacy') loadDiplomacy();
  if (S.drawer === 'decisions') loadDecisions();
  renderDrawer();
}
async function loadResearch() { S.research = await api.get('/api/preview/research'); renderDrawer(); }
async function loadDiplomacy() {
  const ids = S.view.civs.map(c => c.id).filter(id => id !== S.me);
  const res = await Promise.all(ids.map(id => api.get(`/api/preview/diplomacy?with=${id}`)));
  S.diplo = Object.fromEntries(ids.map((id, i) => [id, res[i]])); renderDrawer();
}
function renderDrawer() {
  const el = $('#drawer'); if (!S.drawer || !S.view) return;
  const f = { nation: drawerNation, era: drawerEra, merit: drawerMerit, cities: drawerCities, research: drawerResearch, diplomacy: drawerDiplomacy, market: drawerMarket, chronicle: drawerChronicle, decisions: drawerDecisions }[S.drawer];
  setHtml(el, `<button class="close-x" type="button" data-drawer="${S.drawer}" aria-label="閉じる">×</button>` + f());
}
function drawerCities() {
  const cities = myCities(), units = myUnits();
  const draftUnits = new Set(S.drafts.map(d => d.dto.unit ?? d.dto.army ?? d.dto.settler));
  return `<div class="eyebrow">CITIES & ARMIES</div><h2>都市と軍</h2><p class="drawer-intro">待機中のものには印がつきます。クリックで地図がその場所へ移動します。</p>
    <div class="section-title">都市 ${cities.length}</div>
    ${cities.map(c => `<div class="list-row" data-focus-select="${key(c.q, c.r)}"><div class="main"><div class="title">${c.capital ? '★ ' : ''}${T.cityName(c.id)} <span class="meta">人口${c.pop}</span></div><div class="meta">${(c.queue || []).length ? '生産：' + esc(T.itemName(c.queue[0])) : '<span class="idle">生産予定なし</span>'}</div></div><span class="meta">防御 ${(c.defense / 10).toFixed(0)}/${(c.defenseMax / 10).toFixed(0)}</span></div>`).join('')}
    <div class="section-title">部隊 ${units.length}</div>
    ${units.map(u => `<div class="list-row" data-unit-row="${u.id}"><div class="main"><div class="title">${T.UNIT_GLYPH[u.type]} ${T.UNIT[u.type]} ${u.civilian ? '' : `<span class="meta">兵${(u.troops / 10).toFixed(1)}</span>`}</div><div class="meta">${u.q}, ${u.r} ${u.path?.length ? `· 移動中（残り${u.path.length}）` : draftUnits.has(u.id) ? '· 命令あり' : u.standing ? `· ${T.STANDING_GLYPH[u.standing.kind]} ${esc(T.standingText(u.standing))}` : '· <span class="idle">待機</span>'}</div></div></div>`).join('') || '<p class="desc">部隊はいません。</p>'}`;
}
function drawerResearch() {
  const r = S.research, e = S.view.economy;
  const draft = S.drafts.find(d => d.dto.type === 'SetResearch');
  const queue = draft ? draft.dto.techs : e.researchQueue;
  let html = `<div class="eyebrow">RESEARCH · 研究</div><h2>研究</h2><p class="drawer-intro">科学は毎ティック蓄積され、予定の先頭の技術に使われます。都市が1つ増えるごとに研究コストは10%上がります。予定は最大3件。</p>`;
  if (e.research) html += `<div class="meter"><span>研究中：${T.TECH[e.research.tech]} ${fmt(e.research.store)} / ${fmt(e.research.cost)}</span><div class="bar"><i style="width:${Math.min(100, e.research.store / e.research.cost * 100)}%"></i></div></div>`;
  html += `<div class="section-title">予定 ${draft ? '· 命令あり' : ''}</div>${queue.length ? queue.map((t, i) => `<div class="option queued"><span class="ic">✧</span><span class="name">${i + 1}. ${T.TECH[t]}</span><span></span></div>`).join('') + `<button class="btn" type="button" data-order='${JSON.stringify({ type: 'SetResearch', techs: [] })}'>予定をクリア（枠1）</button>` : '<p class="desc">予定はありません。</p>'}`;
  if (!r) return html + '<p class="desc">計算しています…</p>';
  for (const era of [1, 2, 3, 4]) {
    html += `<div class="tech-era">ERA ${['I', 'II', 'III', 'IV'][era - 1]}</div>`;
    for (const t of r.filter(t => t.era === era)) {
      const inQ = queue.includes(t.tech);
      const planned = new Set([...S.view.economy.techs, ...queue]);
      const prereqOk = t.prereqs.every(p => planned.has(p));
      const dto = { type: 'SetResearch', techs: [...queue, t.tech].slice(0, 3) };
      const can = !t.held && !inQ && prereqOk && queue.length < 3;
      const why = t.held ? '研究済み' : inQ ? '予定に入っています' : !prereqOk ? `先に${t.prereqs.filter(p => !planned.has(p)).map(p => T.TECH[p]).join('・')}が必要` : queue.length >= 3 ? '予定は3件までです' : '';
      html += `<button class="option ${t.held ? 'queued' : ''}" type="button" ${can ? `data-order='${JSON.stringify(dto)}'` : 'aria-disabled="true"'}><span class="ic">${t.held ? '✓' : '✧'}</span><span><span class="name">${T.TECH[t.tech]}</span><div class="meta">解放：${T.TECH_UNLOCK[t.tech]}</div>${why && !t.held ? `<div class="why">${why}</div>` : ''}</span><span class="cost">${fmt(t.cost)}</span></button>`;
    }
  }
  return html;
}
function drawerDiplomacy() {
  const v = S.view;
  let html = `<div class="eyebrow">DIPLOMACY · 外交官の担当</div><h2>外交</h2><p class="drawer-intro">申し入れは次のティック以降に相手が受ければ成立し、6ティックで失効します。講和すると12ティックの休戦になります。<b>宣戦には、外交官とは別の人の将軍か内政官の同意（同じティック）が必要です。</b>${held().includes('Diplomat') ? '' : ' あなたは外交官ではないので、ここでの命令は外交官への献策になります。'}</p>`;
  const inbox = v.proposals.filter(p => p.to === S.me);
  html += `<div class="section-title">届いた申し入れ ${inbox.length}</div>` + (inbox.map(p => {
    const kind = { Peace: ['講和', 'AcceptPeace'], Nap: [`不可侵条約（保証金${p.bond}金ずつ・30ティック）`, 'AcceptNap'], Alliance: ['同盟', 'AcceptAlliance'] }[p.kind];
    const dto = p.kind === 'Nap' ? { type: kind[1], civ: p.from, bond: p.bond } : { type: kind[1], civ: p.from };
    return `<div class="proposal"><div><span class="swatch-s" style="background:${T.CIV_COLORS[p.from]}"></span><b>${esc(civN(p.from))}</b>から${kind[0]}</div><div class="when">ティック${p.tick}に提案 · ティック${p.expires}で失効${p.kind === 'Nap' ? ' · 破った側の保証金は相手のものに' : ''}</div><div class="row" style="margin-top:6px"><button class="btn primary" type="button" data-order='${JSON.stringify(dto)}'>受け入れる（枠1）</button></div></div>`;
  }).join('') || '<p class="desc">ありません。</p>');
  html += `<div class="section-title">文明 · 行をクリックで操作</div>`;
  S.diploOpen ??= new Set();
  for (const c of v.civs.filter(c => c.id !== S.me)) {
    const opts = S.diplo[c.id] || [];
    const persona = T.PERSONA[v.personas?.[c.id]] || '';
    const hasInbox = inbox.some(p => p.from === c.id);
    const open = S.diploOpen.has(c.id) || hasInbox;
    const relTag = `<span class="tag ${c.relation === 'war' ? 'bad' : c.relation === 'alliance' ? 'positive' : c.relation === 'nap' ? 'warning' : ''}">${T.RELATION[c.relation]}</span>`;
    const allowed = opts.filter(o => !o.action.startsWith('Accept'));
    const blocked = allowed.filter(o => o.blocked);
    html += `<details class="civ-row" data-civ-row="${c.id}" ${open ? 'open' : ''}>
      <summary><span class="l1"><span class="swatch-s" style="background:${T.CIV_COLORS[c.id]}"></span><b>${esc(civN(c.id))}</b><span class="badge">第${c.era}時代</span>${hasInbox ? '<span class="tag warning">申し入れ</span>' : ''}<span class="grow"></span>${relTag}</span><span class="l2 meta">国民${(v.members || []).filter(m => m.civ === c.id).length}人 · 確認済み${c.cities}都市 · 目視の兵${c.troopsSeen}</span></summary>
      <div class="civ-body">
        <div class="when">節目 ${c.tiers.map((t, i) => `${PATH_JA[i]}${t}`).join(' · ')} · スターゲート${c.stages}/3${c.aggressor ? ' · <span style="color:var(--bad)">侵略中</span>' : ''}${c.truceUntil > v.tick ? ` · 休戦 ティック${c.truceUntil}まで` : ''}</div>
        <div class="when">あなたの不満 ${c.myGrievanceAgainst ?? 0} · 相手の不満 ${c.grievanceAgainstMe ?? 0}（30以上で正当な開戦理由）</div>
        <div class="row" style="margin-top:6px">${allowed.map(o => {
          const dto = o.action === 'ProposeNap' ? { type: 'ProposeNap', civ: c.id, bond: 30 } : { type: o.action, civ: c.id };
          return o.blocked ? `<button class="btn" type="button" disabled title="${esc(T.blockedText(o.blocked))}">${T.DIPLO_ACTION[o.action]}</button>` : `<button class="btn ${o.action === 'DeclareWar' ? 'danger' : ''}" type="button" data-order='${JSON.stringify(dto)}'>${T.DIPLO_ACTION[o.action]}</button>`;
        }).join('')}${(held().includes('General') || held().includes('Steward')) && c.relation === 'peace' ? `<button class="btn" type="button" data-order='${JSON.stringify({ type: 'ConsentWar', civ: c.id })}' title="外交官の宣戦に、将軍・内政官として同意します（枠を使いません）">⚖ 宣戦に同意</button>` : ''}${c.relation === 'nap' ? `<button class="btn danger" type="button" data-order='${JSON.stringify({ type: 'BreakNap', civ: c.id })}'>条約を破棄（保証金を失い宣戦）</button>` : ''}</div>
        ${blocked.length ? `<div class="when" style="margin-top:4px">${blocked.map(o => `${T.DIPLO_ACTION[o.action].replace(/（.*）/, '')}：${T.blockedText(o.blocked)}`).join('<br>')}</div>` : ''}
      </div></details>`;
  }
  if (v.civs.some(c => c.relation === 'alliance')) html += `<button class="btn wide" type="button" data-order='${JSON.stringify({ type: 'LeaveAlliance' })}'>同盟から離脱する（6ティック後）</button>`;
  return html;
}
function drawerMarket() {
  const v = S.view, e = v.economy;
  const hasCurrency = e.techs.includes('Currency');
  const q = S.marketQuote;
  let html = `<div class="eyebrow">MARKETS · 市場</div><h2>市場</h2><p class="drawer-intro">ゲーム内の金の市場と、プレイヤー同士のUSDC取引所があります。どちらもティックごとに一度だけ、同じ価格でまとめて約定するので、注文の早さは関係ありません。</p>`;
  html += `<div class="section-title">金の市場（鉄・馬 ⇄ 金）</div>`;
  if (!hasCurrency) html += `<div class="explanation">「通貨」の研究が必要です。</div>`;
  html += v.pools.map(p => `<div class="meter"><span>${T.RESOURCE[p.good]}：在庫 ${fmt(p.goods)} · 金 ${fmt(p.gold)} · 現在値 1あたり ${p.spot.toFixed(2)}金</span></div>`).join('');
  html += `<div class="row"><label class="field">品目<select id="amm-good"><option value="Iron">鉄</option><option value="Horses">馬</option></select></label><label class="field">売買<select id="amm-side"><option value="Buy">買う</option><option value="Sell">売る</option></select></label><label class="field">数量<input id="amm-amount" type="number" min="1" max="50" value="5" style="width:64px"></label></div>
    <button class="btn" type="button" id="amm-quote">見積もる</button>`;
  if (q) {
    const impactPct = (q.impact * 100);
    html += `<div class="quote"><span class="big">${q.side === 'Buy' ? '−' : '+'}${Math.abs(q.gold).toFixed(1)}金 → ${q.side === 'Buy' ? '+' : '−'}${q.amount} ${T.RESOURCE[q.good]}</span>
      <span>単価 ${q.price.toFixed(2)}金（現在値 ${q.spot.toFixed(2)}）· 価格への影響 <b style="color:${Math.abs(impactPct) > 5 ? 'var(--bad)' : Math.abs(impactPct) > 1 ? 'var(--warn)' : 'var(--good)'}">${impactPct.toFixed(1)}%</b></span>
      <span>手数料 ${q.fee.toFixed(1)}金（3%：交易拠点の保有者に1%、2%は消滅）</span>
      <span class="meta">同じティックの他の注文とまとめて約定するため、実際の価格は変わります。許容：見積もり±2%</span></div>
      <button class="btn primary" type="button" ${hasCurrency ? '' : 'disabled'} data-order='${JSON.stringify({ type: 'MarketTrade', good: { kind: q.good }, side: q.side, amount: q.amount, limitGold: q.side === 'Buy' ? Math.ceil(Math.abs(q.gold) * 1.02) : Math.floor(Math.abs(q.gold) * .98) })}'>命令に追加（枠1）</button>`;
  }
  const se = v.season || {};
  const tariff = e.tariffBps / 100;
  const deliveries = e.deliveries || [];
  html += `<div class="usdc-box"><div class="usdc-label">USDC 市場 · 国庫どうしの一括競売 · テスト資金</div>
    <p class="desc" style="margin:4px 0">原材料だけを他の国と取引できます（ティックごとに1つの価格でまとめて約定）。買い手は手数料5%と<b>関税</b>を払い、どちらも80%が賞金プール・20%が運営に入ります。関税は国の累計支出とともに上がります。買った物資は${se.deliveryTicks ?? 3}ティック後に届き、富・交易量・功績には数えません。スターゲートを建てている都市には届きません。自国・交戦中の国とは取引できません。${usdc(se.spendConsentUsdc)} USDCを超える支出には、外交官とは別の役職者の同意が必要です。</p>
    <div class="stats"><div class="stat"><div class="k">国庫</div><div class="v">${usdc(e.treasury)}<small> USDC</small></div></div><div class="stat"><div class="k">累計支出</div><div class="v">${usdc(e.marketSpent)}</div></div><div class="stat"><div class="k">いまの関税</div><div class="v">${tariff.toFixed(1)}<small>%</small></div></div></div>
    ${deliveries.length ? `<div class="section-title">輸送中</div>${deliveries.map(d => `<div class="list-row" style="cursor:default"><div class="main"><div class="title">${goodName(d.good)} ${d.qty}</div></div><span class="meta">ティック${d.due}に到着</span></div>`).join('')}` : ''}
    ${!se.market ? '<div class="explanation">このシーズンは市場がオフです。</div>' : v.tick >= (se.marketFreeze ?? 120) ? '<div class="explanation">終盤（ティック120以降）のため凍結中です。</div>' : `
    <div class="row"><label class="field">品目<select id="ex-good"><option value="Gold">金</option><option value="Iron">鉄</option><option value="Horses">馬</option><option value="Food">食料（首都へ）</option><option value="Production">生産（首都へ）</option></select></label>
    <label class="field">売買<select id="ex-side"><option value="Buy">買う</option><option value="Sell">売る</option></select></label></div>
    <div class="row"><label class="field">数量<input id="ex-amount" type="number" min="1" value="1" style="width:64px"></label><label class="field">単価 USDC<input id="ex-price" type="number" min="0.01" step="0.01" value="0.10" style="width:80px"></label></div>
    ${S.exchangeStep === 1 ? `<div class="explanation" id="ex-confirm-text"></div><div class="row"><button class="btn" type="button" id="ex-cancel">戻る</button><button class="btn usdc" type="button" id="ex-confirm">USDCの注文を命令に追加</button></div>` : `<button class="btn usdc" type="button" id="ex-review">内容を確認する</button>`}
    ${(held().some(r => r !== 'Diplomat')) ? `<div class="row" style="margin-top:6px"><button class="btn" type="button" data-order='${JSON.stringify({ type: 'ConsentSpend', usdc: 20_000_000 })}'>⚖ 外交官の支出に同意（20 USDCまで）</button></div>` : ''}`}
    <p class="meta" style="margin:6px 0 0">取引所の注文は命令の枠を使いません。賞金プール（テスト）：${usdc(v.projection?.pool)} USDC</p></div>`;
  return html;
}
// ------------------------------------------------------------------ V5: nation, era, merit
const who = m => (m ? `${esc(m.name)}${m.kind === 'agent' ? ' <span class="badge agent">AI</span>' : ''}${m.attested ? ' <span class="badge agent" title="登録の証明あり（ERC-8004 など）">証明済みAI</span>' : ''}` : '<span class="meta">代行（AI）</span>');

/** The nation plaza (V5 §12): offices, election, proposals, recalls. */
function drawerNation() {
  const v = S.view, g = v.gov; if (!g) return '<p class="desc">読み込んでいます…</p>';
  const me = S.member, mine = held();
  const roster = v.roster || [];
  const myVotes = S.myVotes || {};
  let html = `<div class="eyebrow">NATION · 国の広場</div><h2>${esc(civN(S.me))}の政府</h2>
    <p class="drawer-intro">国民は${g.members}人。役職者だけが担当の命令を出せます。国民は誰でも献策・支持・投票・リコールができます（すべてチェーンに記録されます）。</p>
    <div class="section-title">役職者</div>`;
  html += g.offices.map(o => `<div class="office-row ${o.holder?.id === me ? 'me' : ''}"><span class="og">${ROLE_GLYPH[o.role]}</span><div class="main"><div class="title">${ROLE_JA[o.role]} · ${who(o.holder)}${o.holder?.id === me ? ' <span class="tag positive">あなた</span>' : ''}</div>
      <div class="meta">${o.holder ? `ティック${o.since}から · ${o.active ? '活動中' : `<span class="idle">最後の命令 ${o.lastAct ?? 'なし'}</span>`}` : '立候補がないため代行が務めます'}${o.runnerUp ? ` · 次点 ${esc(o.runnerUp.name)}` : ''}</div></div>
      ${o.holder && o.holder.id !== me ? `<button class="btn" type="button" data-gov='${esc(JSON.stringify({ type: 'Recall', role: o.role }))}' title="直近10ティックに活動した国民の過半数が5ティック以内に賛成すると解任">リコール</button>` : ''}</div>`).join('');
  if (g.recalls.length) html += `<div class="section-title">リコール投票中</div>` + g.recalls.map(r => `<div class="proposal"><b>${ROLE_JA[r.role]}</b>（${esc(r.holder?.name || '')}）のリコール${r.automatic ? '（30ティック命令なしのため自動）' : ''}<div class="when">賛成 ${r.yes} / 必要 ${Math.floor(g.electorate / 2) + 1}（活動中の国民${g.electorate}人の過半数） · ティック${r.closes}まで</div><div class="row" style="margin-top:6px"><button class="btn danger" type="button" data-gov='${esc(JSON.stringify({ type: 'Recall', role: r.role }))}'>賛成する</button></div></div>`).join('');
  html += `<div class="section-title">選挙 · 次の任期はティック${g.nextElection ?? '—'}から${g.voteOpen ? ' · <span class="tag warning">投票受付中</span>' : ` · 投票はティック${g.voteFrom ?? '—'}から`}</div>`;
  const standing = v.member?.standingFor || [];
  html += `<div class="row">${ROLES.map(r => `<button class="btn ${standing.includes(r) ? 'primary' : ''}" type="button" data-stand="${r}">${ROLE_GLYPH[r]} ${ROLE_JA[r]}に${standing.includes(r) ? '立候補中' : '立候補'}</button>`).join('')}</div><p class="meta">1人が兼ねられる役職は2つまで。立候補はいつでも変えられます。</p>`;
  html += g.candidates.map(c => `<div class="cand"><span class="og">${ROLE_GLYPH[c.role]}</span><b>${ROLE_JA[c.role]}</b> ${c.candidates.length ? c.candidates.map(x => `<span class="cand-pill">${who(x.member)} <small>${x.votes}票</small>${g.voteOpen ? `<button class="btn ${myVotes[c.role] === x.member.id ? 'primary' : ''}" type="button" data-gov='${esc(JSON.stringify({ type: 'Vote', role: c.role, candidate: x.member.id }))}' data-vote-role="${c.role}" data-vote-for="${x.member.id}">投票</button>` : ''}</span>`).join('') : '<span class="meta">立候補者なし（代行）</span>'}</div>`).join('');
  html += `<div class="section-title">献策 · 支持の多い順</div>`;
  const props = [...g.proposals].sort((a, b) => b.supporters - a.supporters);
  html += props.length ? props.map(p => {
    const canAdopt = mine.includes(p.role);
    const adopted = (S.adopt?.[p.role] || []).includes(p.id);
    const supported = (p.supportedBy || []).includes(me);
    return `<div class="proposal"><div><span class="og">${ROLE_GLYPH[p.role]}</span> <b>${ROLE_JA[p.role]}へ</b> · ${who(p.proposer)} · 支持 ${p.supporters}</div>
      <ul class="prop-orders">${p.orders.map(o => `<li>${esc(describeOrder(o).label)}</li>`).join('')}</ul>
      <div class="when">ティック${p.tick}に提出 · ティック${p.expires}で失効 · 採用されると功績を献策者と役職者で半分ずつ</div>
      <div class="row" style="margin-top:6px">${p.proposer?.id !== me ? `<button class="btn ${supported ? 'primary' : ''}" type="button" ${supported ? 'disabled' : `data-gov='${esc(JSON.stringify({ type: 'Support', proposal: p.id }))}'`}>${supported ? '支持済み' : '支持する'}</button>` : '<span class="meta">あなたの献策</span>'}
      ${canAdopt ? `<button class="btn ${adopted ? 'primary' : ''}" type="button" data-adopt="${p.role}:${p.id}">${adopted ? '採用予定 ✓（確定で送信）' : '採用する'}</button>` : ''}</div></div>`;
  }).join('') : '<p class="desc">献策はまだありません。地図で担当外の命令を選んで確定すると、その役職への献策になります。</p>';
  html += `<div class="section-title">国民 ${roster.length}人</div>` + roster.map(m => `<div class="list-row" style="cursor:default"><div class="main"><div class="title">${who(m)}${m.id === me ? ' <span class="tag positive">あなた</span>' : ''}</div><div class="meta">功績 ${m.merit.toFixed(1)} · 活動 ${m.activeWindows}/18区間${m.active ? ' ✓' : ''}</div></div></div>`).join('');
  return html;
}

/** Milestones of the four paths and the eras (V5 §6). */
function drawerEra() {
  const v = S.view, A = v.achievements; if (!A) return '';
  const th = A.thresholds, mine = A.nations[S.me], f = v.facts || {};
  const tierText = [
    k => `領土 ${th.hegemonyTiles[k]}${th.hegemonyCities[k] ? (k === 2 ? ` または占領都市${th.hegemonyCities[k]}` : ` ＋ 占領都市${th.hegemonyCities[k]}`) : ''}`,
    k => `人口 ${th.prosperityPop[k]}${th.prosperityWealth[k] ? ` ＋ 富 ${fmt(th.prosperityWealth[k])}` : ''}`,
    k => (k < 3 ? `技術 ${th.scienceTechs[k]}` : k === 3 ? 'スターゲート I' : 'スターゲート III'),
    k => ['条約相手1 または 使節', '条約相手1 ＋ 宗主経験', '宗主 ＋ 条約相手1', `宗主 ＋ 同盟 ＋ 交易 ${th.concordTrade[0]}`, `宗主2 ＋ 条約相手2 ＋ 交易 ${th.concordTrade[1]}`][k],
  ];
  const now = [`領土 ${f.tiles} · 占領都市 ${f.capturedHeld}`, `人口 ${f.pop} · 富 ${fmt(f.wealth)}`, `技術 ${f.techs} · スターゲート最高 ${f.starGateMax}`, `条約相手 ${f.partners} · 同盟 ${f.alliances} · 宗主 ${f.suzerainties}${f.everSuzerain ? '（経験あり）' : ''} · 交易 ${fmt(f.trade)}`];
  let html = `<div class="eyebrow">ERAS · 4つの道と時代</div><h2>${esc(civN(S.me))} · 第${mine.era}時代 · ${mine.points}点</h2>
    <p class="drawer-intro">節目を1つ達成するごとに点が入り、同じ段階の節目を2つの道（第5段階は3つ）で達成すると新しい時代に入ります。領土・人口・宗主・条約・占領都市は<b>シーズン終了時の状態</b>で判定するので、失えば取り消されます（下は「今終わったら」）。賞金プールは国民のいる国の点の比で分けます。</p>
    <table class="grid era-grid"><thead><tr><th>段階</th>${PATH_JA.map(n => `<th>${n}</th>`).join('')}</tr></thead><tbody>`;
  for (let k = 0; k < 5; k++) {
    html += `<tr><td><b>${k + 1}</b><small> ${A.tierPoints[k]}点</small></td>${PATH_JA.map((_, p) => `<td class="${mine.tiers[p] > k ? 'met' : mine.tiers[p] === k ? 'next' : ''}">${mine.tiers[p] > k ? '✓ ' : ''}${esc(tierText[p](k))}</td>`).join('')}</tr>`;
  }
  html += `</tbody></table><div class="meta" style="margin:6px 0">いま：${now.map((t, i) => `<b>${PATH_JA[i]}</b> ${t}`).join(' · ')}</div>`;
  const next = mine.era + 1;
  if (next <= 5) html += `<div class="explanation">第${next}時代まで：段階${next}の節目を${next === 5 ? 3 : 2}つの道で（いま${mine.tiers.filter(t => t >= next).length}つ）。</div>`;
  html += `<div class="section-title">国ごとの達成と賞金の見込み</div><table class="grid"><thead><tr><th>国</th><th>時代</th><th>節目</th><th>点</th><th>取り分</th></tr></thead><tbody>`;
  const pr = v.projection;
  html += A.nations.map(n => `<tr class="${n.civ === S.me ? 'me' : ''}"><td><span class="swatch-s" style="background:${T.CIV_COLORS[n.civ]}"></span>${esc(civN(n.civ))}${pr?.counted?.[n.civ] ? '' : ' <span class="meta" title="国民がいない・活動した国民がいない・都市がない国は配分の対象外">対象外</span>'}</td><td>${n.era}</td><td>${n.tiers.join('/')}</td><td>${n.points}</td><td>${pr ? usdc(pr.nationShare[n.civ]) : '—'}</td></tr>`).join('');
  html += `</tbody></table>`;
  return html;
}

/** My merit and the payout if the season ended now (V5 §7). */
function drawerMerit() {
  const v = S.view, m = v.member, pr = v.projection;
  if (!m) return `<div class="eyebrow">MERIT</div><h2>功績</h2><p class="desc">観戦中は表示されません。</p>`;
  const share = pr?.nationShare?.[S.me] ?? 0, eq = pr?.equalEach?.[S.me] ?? 0;
  const bars = ['hegemony', 'prosperity', 'science', 'concord', 'common'];
  const names = { hegemony: '覇権', prosperity: '繁栄', science: '科学', concord: '協調', common: '役職の務め' };
  const max = Math.max(1, ...bars.map(b => m.merit[b]));
  let html = `<div class="eyebrow">MERIT · 功績と見込み</div><h2>今終わったら ${usdc(m.projectedPayout)} USDC</h2>
    <p class="drawer-intro">国の取り分は ${usdc(share)} USDC。その20%は活動した国民で均等（1人 ${usdc(eq)}・上限は参加費の半分）、残りは功績の比で、国が点を取った道の功績ほど重く分けます。活動した国民＝18区間のうち${m.windowsNeeded}区間以上で命令・献策・支持・投票をした人（あなた：${m.activeWindows}区間${m.active ? ' ✓' : ''}）。</p>
    <div class="section-title">あなたの功績 · 合計 ${m.merit.total.toFixed(1)}</div>`;
  html += bars.map(b => `<div class="meter"><span>${names[b]} ${m.merit[b].toFixed(1)}</span><div class="bar"><i style="width:${(100 * m.merit[b] / max).toFixed(0)}%"></i></div></div>`).join('');
  html += `<div class="section-title">直前のティックで得た功績</div>` + (m.meritLog.length ? m.meritLog.map(e => `<div class="list-row" style="cursor:default"><div class="main"><div class="title">+${e.merit.toFixed(1)} ${names[e.path.toLowerCase()] || e.path}</div><div class="meta">${esc(T.MERIT_WHAT?.[e.what] || e.what)}</div></div></div>`).join('') : '<p class="desc">なし。都市の成長・建物の完成・研究・占領・条約・交易などで、命令を出した人（採用された献策は半分ずつ）に付きます。</p>');
  return html;
}
function drawerChronicle() {
  const f = S.chronFilter || 'all';
  const groups = { all: null, war: ['war', 'capture', 'raze', 'revolt'], diplo: ['peace', 'ally', 'diplo'], growth: ['found', 'science', 'tech'] };
  const rows = (S.view.chronicle || []).map(e => ({ tick: e.tick, kt: T.chronicleText(e.text) })).filter(e => !groups[f] || groups[f].includes(e.kt[0]));
  return `<div class="eyebrow">CHRONICLE · 年代記</div><h2>年代記</h2>
    <div class="row">${[['all', 'すべて'], ['war', '戦い'], ['diplo', '外交'], ['growth', '発展']].map(([k, n]) => `<button class="btn ${f === k ? 'primary' : ''}" type="button" data-chron="${k}">${n}</button>`).join('')}</div>
    <div style="margin-top:8px">${rows.map(e => `<div class="list-row" style="cursor:default"><div class="main"><div class="title" style="font-weight:400">${T.KIND_GLYPH[e.kt[0]] || '·'} ${esc(e.kt[1])}</div></div><span class="meta">t${e.tick}</span></div>`).join('') || '<p class="desc">まだ記録はありません。</p>'}</div>`;
}
// ------------------------------------------------------------------ decision log
async function loadDecisions() {
  const d = await api.get('/api/decisions?limit=240');
  S.decisions = d; S.verified ??= {};
  renderDrawer();
  for (const r of d.records) {
    const k = `${r.tick}:${r.civ}:${r.digest}`;
    if (r.reveal && S.verified[k] === undefined) { S.verified[k] = await V.verifyRecord(r); }
  }
  renderDrawer();
}
async function proveTile() {
  const t = S.tile && map.tiles.get(S.tile);
  if (!t) { S.proof = { error: '先に地図でマスを選んでください。' }; return renderDrawer(); }
  const civId = +(S.proofCiv ?? S.me), tick = +(S.proofTick ?? Math.max(0, S.view.tick - 1));
  const p = await api.get(`/api/decisions/proof?tick=${tick}&civ=${civId}&kind=tile&id=${t.index}`);
  if (!p.ok) { S.proof = { error: p.error === 'not available' ? 'まだ解決していないティックは証明できません。' : '古い観測は根（ルート）しか残っていません。' }; return renderDrawer(); }
  const leafOk = await V.verifyProof(p);
  const rec = (S.decisions?.records || []).find(r => r.tick === tick && r.civ === civId);
  const digestOk = rec?.reveal ? await V.verifyRecord(rec) : null;
  S.proof = { tick, civ: civId, tile: V.decodeTile(p.body), steps: p.proof.length, root: p.root, leafOk, rootMatches: rec ? rec.obsRoot === p.root : null, digestOk, text: rec?.reveal?.text };
  renderDrawer();
}
function drawerDecisions() {
  const d = S.decisions; const civs = S.view.civs;
  const who = S.decFilter ?? 'all';
  const fogName = ['未踏（見えない）', '霧の中（記憶のみ）', '視界の中'];
  const pr = S.proof; const pt = S.tile && map.tiles.get(S.tile);
  const check = ok => ok === true ? '<span class="vchk ok">✓</span>' : ok === false ? '<span class="vchk bad">✗</span>' : '<span class="vchk">…</span>';
  let html = `<div class="eyebrow">DECISION LOG · 判断ログ</div><h2>判断の証拠</h2>
    <p class="drawer-intro">全文明が毎ティック、解決<b>前</b>に「見えていた世界（観測ルート）・方針・理由」を1つのハッシュで約束し、解決<b>後</b>に理由を公開します。下の ✓ はサーバーではなく、このブラウザが SHA-256 で再計算した結果です。</p>
    <div class="section-title">観測の証明 · そのとき何が見えていたか</div>
    <div class="proof-tool">
      <label>文明 <select id="proof-civ">${civs.map(c => `<option value="${c.id}" ${+(S.proofCiv ?? S.me) === c.id ? 'selected' : ''}>${esc(civN(c.id))}</option>`).join('')}</select></label>
      <label>ティック <input id="proof-tick" type="number" min="0" max="${Math.max(0, S.view.tick - 1)}" value="${S.proofTick ?? Math.max(0, S.view.tick - 1)}"></label>
      <span class="meta">マス：${pt ? `${pt.q}, ${pt.r}` : '地図で選択'}</span>
      <button class="btn primary" type="button" id="prove-tile" ${pt ? '' : 'disabled'}>証明する</button>
    </div>
    ${pr?.error ? `<div class="explanation">${esc(pr.error)}</div>` : pr ? `<div class="proof-result">
      <div class="pr-head">ティック${pr.tick}の<b>${esc(civN(pr.civ))}</b>から見たマス (${pr.tile.q}, ${pr.tile.r})：<b>${fogName[pr.tile.fog]}</b>${pr.tile.ownerCity !== null ? ` · ${esc(T.cityName(pr.tile.ownerCity))}の領土と認識` : ''}</div>
      <div class="pr-row">${check(pr.leafOk)} このマスの葉 → 観測ルート（経路${pr.steps}段）</div>
      <div class="pr-row">${check(pr.rootMatches)} 観測ルートが約束に使われたものと一致 <code>${pr.root.slice(0, 10)}…</code></div>
      <div class="pr-row">${check(pr.digestOk)} ${pr.digestOk === null ? '理由はまだ非公開です（次のティックで公開され、約束と照合できます）' : `公開された理由が約束（digest）と一致${pr.text ? `：「${esc(pr.text)}」` : '（理由の記入なし）'}`}</div>
      ${pr.tile.fog < 2 ? '<p class="desc">このマスは判断のとき見えていませんでした。ここにいた部隊について、この文明は知り得なかったことになります。</p>' : ''}
    </div>` : ''}
    <div class="section-title">タイムライン</div>
    <div class="row dec-filter"><button class="btn ${who === 'all' ? 'primary' : ''}" type="button" data-dec="all">全員</button>${civs.map(c => `<button class="btn ${+who === c.id && who !== 'all' ? 'primary' : ''}" type="button" data-dec="${c.id}"><span class="swatch-s" style="background:${T.CIV_COLORS[c.id]}"></span>${esc(civN(c.id))}</button>`).join('')}</div>`;
  if (!d) return html + '<p class="desc">読み込んでいます…</p>';
  const rows = d.records.filter(r => who === 'all' || r.civ === +who);
  let last = null;
  html += '<div class="dec-list">';
  for (const r of rows.slice(0, 120)) {
    if (r.tick !== last) { html += `<div class="dec-tick">ティック ${r.tick}${r.tick >= d.open ? ' · 解決待ち' : ''}</div>`; last = r.tick; }
    const k = `${r.tick}:${r.civ}:${r.digest}`; const ok = S.verified?.[k];
    const agent = civ(r.civ)?.kind !== 'Human';
    html += `<div class="dec-row"><div class="dec-who"><span class="swatch-s" style="background:${T.CIV_COLORS[r.civ]}"></span><b>${esc(civN(r.civ))}</b><span class="badge ${agent ? 'agent' : 'human'}">${agent ? 'AGENT' : 'HUMAN'}</span>${r.policy ? `<code>${esc(r.policy)}</code>` : ''}</div>
      <div class="dec-steps"><span title="decision_digest ${r.digest}">約束 <code>${r.digest.slice(0, 8)}</code></span><span class="arrow">→</span>${r.reveal ? `<span>公開 t${r.reveal.at}</span><span class="arrow">→</span>${ok === true ? '<span class="vchk ok">✓ 一致</span>' : ok === false ? '<span class="vchk bad">✗ 不一致</span>' : '<span class="vchk">検証中</span>'}` : '<span class="meta">解決後に公開</span>'}</div>
      ${r.reveal ? `<div class="dec-text">${r.reveal.text ? esc(r.reveal.text) : '<span class="meta">（理由の記入なし）</span>'}</div>` : ''}</div>`;
  }
  html += rows.length ? '</div>' : '<p class="desc">まだ記録はありません。ティックが進むと並びます。</p></div>';
  return html;
}

function renderNavDots() {
  const v = S.view;
  const inbox = v.proposals.filter(p => p.to === S.me).length;
  const idle = myCities().filter(c => !(c.queue || []).length).length;
  const set = (id, n) => { const el = $(id); if (el) { el.hidden = !n; el.textContent = n; } };
  const g = v.gov || {};
  set('#dot-diplomacy', inbox); set('#dot-cities', idle); set('#dot-research', v.economy?.researchQueue.length ? 0 : 1);
  set('#dot-nation', (g.recalls?.length || 0) + (g.voteOpen ? 1 : 0) + (g.proposals || []).filter(p => held().includes(p.role)).length);
}
function renderMinimap() {
  const v = S.view;
  S.mini = drawMinimap($('#minimap'), S.map, v, map.viewport());
  const mine = v.civs[S.me];
  $('#mini-left').textContent = `${S.map.tiles.length}マス · 6文明`;
  $('#mini-right').textContent = `領土 ${[...v.owners].filter(c => c === String(S.me)).length}`;
}

// ================================================================== events
document.addEventListener('toggle', e => {
  const row = e.target.closest?.('[data-civ-row]'); if (!row) return;
  S.diploOpen ??= new Set(); const id = +row.dataset.civRow;
  if (row.open) S.diploOpen.add(id); else S.diploOpen.delete(id);
}, true);
document.addEventListener('click', async e => {
  const t = e.target.closest('button, [data-focus], [data-focus-select], [data-unit-row], [data-chip]');
  if (!t) return;
  const d = t.dataset;
  if (d.order) { try { if (addDraft(JSON.parse(d.order))) { renderInspector(); renderDrawer(); } } catch (err) { console.error(err); } return; }
  if (d.remove !== undefined) { removeDraft(+d.remove); return; }
  if (d.gov) { await govAction(JSON.parse(d.gov), t); return; }
  if (d.stand) {
    const cur = S.view.member?.standingFor || [];
    const roles = cur.includes(d.stand) ? cur.filter(r => r !== d.stand) : [...cur, d.stand];
    await govAction({ type: 'Stand', roles }, t); return;
  }
  if (d.adopt) {
    const [role, id] = d.adopt.split(':'); S.adopt ??= {}; const list = S.adopt[role] ||= [];
    const i = list.indexOf(+id); if (i >= 0) list.splice(i, 1); else list.push(+id);
    S.committedJson = ''; renderDrawer(); renderDock(); return;
  }
  if (d.chip !== undefined) { const dr = S.drafts[+d.chip]; if (dr?.focus) map.focusTile(dr.focus); return; }
  if (d.drawer) { openDrawer(d.drawer); return; }
  if (d.close !== undefined) { S.tile = null; S.unit = null; map.setSelection(null); map.setOverlay({}); renderInspector(); return; }
  if (d.focus) { map.focusTile(d.focus); selectTile(d.focus); return; }
  if (d.focusSelect) { map.focusTile(d.focusSelect); selectTile(d.focusSelect); return; }
  if (d.unitRow) { const u = S.view.units.find(x => x.id === +d.unitRow); if (u) { map.focusTile(key(u.q, u.r)); selectUnit(u.id); } return; }
  if (d.selectUnit) { const u = S.view.units.find(x => x.id === +d.selectUnit); if (u) { map.focusTile(key(u.q, u.r)); selectUnit(u.id); } return; }
  if (d.move) { quickMove(d.move); return; }
  if (d.queueClear) { addDraft({ type: 'SetQueue', city: +d.queueClear, items: [] }); renderInspector(); return; }
  if (d.chron) { S.chronFilter = d.chron; renderDrawer(); return; }
  if (d.dec) { S.decFilter = d.dec; renderDrawer(); return; }
  if (d.patrolRemove !== undefined && S.patrol) { S.patrol.route.splice(+d.patrolRemove, 1); map.setPatrol(S.patrol); renderInspector(); return; }
  if (d.closeHelp !== undefined) { $('#help').close(); return; }
  if (d.start !== undefined) {
    $('#help').close();
    if (S.view?.phase === 'lobby') { const r = await api.post('/api/start', {}); if (!r.ok) toast(r.error || '開幕できませんでした', 'error'); }
    else await api.post('/api/control', { paused: false });
    poll(); return;
  }
  switch (t.id) {
    // Nothing drafted: the button ends the turn (empty sealed batches on chain).
    case 'commit-btn': if (!isDirty() && held().length && !S.view.member?.ready) endTurn(); else commit(); break;
    case 'pause-btn': await api.post('/api/control', { paused: !S.view.paused }); poll(); break;
    case 'advance-btn': {
      if (isDirty()) await commit(true);
      const r = await api.post('/api/control', { advance: true });
      if (r.waitingFor?.length) toast(`手番を終えました。${r.waitingFor.map(civN).join('・')}の手番終了を待っています。`);
      poll(); break;
    }
    case 'help-btn': $('#help').showModal(); break;
    case 'zoom-in': map.zoomBy(1.2); break;
    case 'zoom-out': map.zoomBy(1 / 1.2); break;
    case 'home': map.focusHome(); break;
    case 'next-skip': S.nextIndex++; renderNext(); break;
    case 'next-go': goNext(false); break;
    case 'summary-close': S.summaryOpen = false; renderSummary(); break;
    case 'prove-tile': proveTile(); break;
    case 'patrol-start': { const u = S.view.units.find(x => x.id === +t.dataset.unit); S.patrol = { unit: u.id, from: [u.q, u.r], route: [] }; map.setPatrol(S.patrol); renderInspector(); toast('地図で巡回する地点を順にクリックしてください（最大6）。'); break; }
    case 'patrol-cancel': S.patrol = null; map.setPatrol(null); renderInspector(); break;
    case 'patrol-done': { const dto = JSON.parse(t.dataset.dto); S.patrol = null; map.setPatrol(null); addDraft(dto); renderInspector(); break; }
    case 'summary-toggle': {
      const wasCollapsed = S.summaryCollapsed && (S.tile !== null || S.unit !== null);
      S.summaryCollapsed = !wasCollapsed; S.summaryPinned = wasCollapsed; renderSummary(); break;
    }
    case 'amm-quote': {
      const good = $('#amm-good').value, side = $('#amm-side').value, amount = Math.max(1, +$('#amm-amount').value || 1);
      const qt = await api.get(`/api/preview/amm?good=${good}&side=${side}&amount=${amount}`);
      S.marketQuote = qt.filled ? { ...qt, good, side, amount } : null;
      if (!qt.filled) toast('その数量は市場の在庫を超えています。', 'error');
      renderDrawer(); break;
    }
    case 'ex-review': {
      S.exchangeStep = 1; renderDrawer();
      const amount = +$('#ex-amount').value, price = +$('#ex-price').value, side = $('#ex-side').value, g = $('#ex-good').value;
      const total = amount * price, fee = side === 'Buy' ? total * .05 : 0;
      $('#ex-confirm-text').innerHTML = `<b>${side === 'Buy' ? '買い' : '売り'}</b>：${goodName({ kind: g })} ${amount} × ${price.toFixed(2)} USDC = ${total.toFixed(2)} USDC${side === 'Buy' ? `＋手数料 ${fee.toFixed(2)}（うち賞金プールへ ${(fee * .8).toFixed(2)}）` : ''}。<br>このティックの締切で、価格が合う相手がいれば約定します。テスト用のUSDCです。`;
      S.exDraft = { amount, price, side, g };
      break;
    }
    case 'ex-cancel': S.exchangeStep = 0; renderDrawer(); break;
    case 'ex-confirm': {
      const { amount, price, side, g } = S.exDraft; const capital = civ(S.me).capital;
      const good = g === 'Food' || g === 'Production' ? { kind: g, city: capital } : { kind: g };
      if (addDraft({ type: 'ExchangeOrder', good, side, amount, price: Math.round(price * 1e6) })) toast('USDCの注文を命令に追加しました（枠は使いません）。');
      S.exchangeStep = 0; renderDrawer(); break;
    }
  }
});
$('#minimap').addEventListener('click', e => {
  if (!S.mini) return; const r = e.currentTarget.getBoundingClientRect();
  const w = S.mini.toWorld(e.clientX - r.left, e.clientY - r.top); map.centerOnWorld(w.x, w.y);
});
document.querySelectorAll('.lens-bar button').forEach(b => b.addEventListener('click', () => {
  S.lens = b.dataset.lens; map.setLens(S.lens);
  document.querySelectorAll('.lens-bar button').forEach(x => x.setAttribute('aria-pressed', String(x === b)));
  renderLensLegend();
}));
function renderLensLegend() {
  const el = $('#lens-legend'); const dot = c => `<i style="background:${c}"></i>`;
  const civs = S.view?.civs || [];
  const body = {
    political: civs.map(c => `<span>${dot(T.CIV_COLORS[c.id])}${esc(civN(c.id))}${c.id === S.me ? '（あなた）' : ''}</span>`).join('') + '<span class="hint">斜線＝戦争中の相手</span>',
    yields: `<span>${dot('#7f9a3e')}食料</span><span>${dot('#9a6b3c')}生産</span><span>${dot('#c9a43f')}金</span><span class="hint">都市が使う土地の基本産出（川・資源込み）</span>`,
    military: `<span>${dot('rgba(172,98,81,.6)')}戦争中の敵軍・蛮族から2マス以内</span>`,
    concord: `<span>${dot('#8a9a8a')}都市国家の周囲2マス（宗主の色）</span><span class="hint">協調の道：宗主・条約・交易</span>`,
  }[S.lens];
  el.hidden = !body; if (body) el.innerHTML = body;
}
document.addEventListener('change', e => {
  if (e.target.id === 'proof-civ') { S.proofCiv = +e.target.value; S.proof = null; renderDrawer(); }
  if (e.target.id === 'proof-tick') { S.proofTick = Math.max(0, Math.min(S.view.tick - 1, +e.target.value || 0)); S.proof = null; renderDrawer(); }
});
$('#rationale').addEventListener('input', () => renderDock());
$('#rationale').addEventListener('keydown', e => { if (e.key === 'Enter') { e.preventDefault(); commit(); } });
document.addEventListener('keydown', e => {
  if (e.target.closest('input, select, textarea') || document.querySelector('dialog[open]')) return;
  if (e.key === ' ') { e.preventDefault(); goNext(true); renderNext(); }
  else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); commit(); }
  else if (e.key === 'Escape' && S.patrol) { S.patrol = null; map.setPatrol(null); renderInspector(); }
  else if (e.key === 'Escape') { S.tile = null; S.unit = null; map.setSelection(null); map.setOverlay({}); if (S.drawer) openDrawer(S.drawer); renderInspector(); }
  else if (e.key.toLowerCase() === 'h') map.focusHome();
  else if (e.key === '`') toggleChain();
});
$('#chain-beat').addEventListener('click', toggleChain);
$('#next-turn').addEventListener('click', nextTurnClick);
$('#tracker').addEventListener('click', () => openDrawer('era'));
$('#notif-stack').addEventListener('click', e => { const b = e.target.closest('[data-n]'); if (b) { S.nextIndex = +b.dataset.n; renderNext(); goNext(false); } });
setInterval(() => { if (S.view) renderMinimap(); }, 1000);

boot();


// ================================================================== V4.1 HUD (Civ benchmark + chain layer)
const KIND_TAG = { human: ['人', ''], external: ['AI', 'ai'], bot: ['BOT', 'bot'] };
const REL_ICON = { war: '⚔', alliance: '🤝', nap: '☮', peace: '' };
const short = s => (s ? `${s.slice(0, 4)}…${s.slice(-4)}` : '');

/** Leader ribbon, as in Civ's top-right: every nation, its era, members, officers. */
function renderRibbon() {
  const v = S.view, ms = v.members || [];
  setHtml($('#ribbon'), v.civs.map(c => {
    const me = c.id === S.me, rel = me ? '' : REL_ICON[c.relation] || '';
    const members = ms.filter(m => m.civ === c.id);
    const humans = members.filter(m => m.host === 'human').length;
    const ach = v.achievements?.nations?.[c.id];
    return `<button class="leader ${me ? 'me' : ''}" type="button" style="--c:${T.CIV_COLORS[c.id % T.CIV_COLORS.length]}" aria-label="${esc(civN(c.id))}">
      ${esc(civN(c.id).slice(0, 1))}
      ${rel ? `<span class="rel">${rel}</span>` : ''}<span class="era">${c.era}</span>
      <span class="kind ${members.length ? (humans ? '' : 'ai') : 'bot'}">${members.length ? `${members.length}人` : '代行'}</span>
      <span class="card"><b>${esc(civN(c.id))}</b>
        <div class="row2"><span>第${c.era}時代${ach ? ` · ${ach.points}点` : ''}</span><span>${me ? 'あなたの国' : esc(T.RELATION[c.relation] || '')}</span></div>
        <div class="row2"><span>国民 ${members.length}人（人間${humans}）</span><span>都市 ${c.cities}</span></div>
        <div class="row2"><span>節目 ${c.tiers.map((t, i) => `${PATH_JA[i]}${t}`).join(' ')}</span></div>
        ${v.projection && members.length ? `<div class="row2"><span>今の取り分</span><span>${usdc(v.projection.nationShare[c.id])} USDC（1人 ${usdc(v.projection.nationShare[c.id] / members.length)}）</span></div>` : ''}
      </span></button>`;
  }).join(''));
}

/** Four paths × five tiers for my nation, and the era (Civ VII legacy paths). */
function renderTracker() {
  const v = S.view, ach = v.achievements?.nations?.[S.me];
  if (!ach) return;
  const pool = poolUsdc();
  setHtml($('#tracker'), `<div class="head"><span>4つの道 · 第${ach.era}時代 · ${ach.points}点</span>${pool ? `<b>賞金 ${fmt(pool)} USDC</b>` : ''}</div>` + ach.tiers.map((t, i) => `<div class="trk"><span class="n">${PATH_JA[i]}</span><span class="steps">${[1, 2, 3, 4, 5].map(k => `<i class="${t >= k ? 'on' : ''}"></i>`).join('')}</span><span class="rank">${t}/5</span></div>`).join(''));
}

/** Civ's Next Turn button: names what still needs a decision, or ends the turn. */
function renderNextTurn() {
  const v = S.view, el = $('#next-turn'); if (!v) return;
  const items = nextItems().filter(i => !i.none);
  const done = v.member?.ready;
  const secs = Math.max(0, v.secondsLeft - (performance.now() - (S.clockAt || performance.now())) / 1000);
  const frac = v.paused ? 1 : v.tickSeconds ? secs / v.tickSeconds : 1;
  const C = 2 * Math.PI * 62;
  const ring = `<svg viewBox="0 0 132 132" aria-hidden="true"><circle cx="66" cy="66" r="62" fill="none" stroke="#ffffff22" stroke-width="5"/><circle cx="66" cy="66" r="62" fill="none" stroke="${frac < .15 ? '#e0674f' : '#f0cf7a'}" stroke-width="5" stroke-linecap="round" stroke-dasharray="${C}" stroke-dashoffset="${C * (1 - frac)}"/></svg>`;
  let cls = '', title, sub;
  if (v.over) { title = 'シーズン終了'; sub = '結果を見る'; }
  else if (v.phase === 'lobby') { title = '開幕する'; sub = '第1回選挙を行い、ティックを始める'; }
  else if (done) { cls = 'done'; const waiting = (v.members || []).filter(s => s.host === 'human' && s.claimed && !s.ready).length; title = '手番を終えた'; sub = waiting ? `ほか${waiting}人を待っています` : '解決を待っています'; }
  else if (items.length) { cls = 'blocker'; title = shortBlocker(items[0]); sub = items.length > 1 ? `ほか${items.length - 1}件 · クリックで移動` : 'クリックで移動'; }
  else { title = '手番を終える'; sub = v.chain ? '署名してチェーンへ送信' : `${Math.ceil(secs)}秒後に自動で解決`; }
  el.className = `next-turn ${cls}`;
  setHtml(el, `${ring}<span class="lbl"><b>${esc(title)}</b><small>${esc(sub)}</small></span>`);
}
/** Civ-style blocker labels: a verb, not a sentence. */
function shortBlocker(it) {
  if (it.drawer === 'research') return '研究を選ぶ';
  if (it.drawer === 'diplomacy') return '外交に返答';
  if (it.tile) return '生産を選ぶ';
  if (it.unit !== undefined) return S.view.units.find(u => u.id === it.unit)?.type === 'Settler' ? '開拓者に命令' : '部隊に命令';
  return it.title;
}
/** End this member's turn now: every office held seals what it has (possibly nothing). */
async function endTurn() {
  const r = await api.post('/api/control', { advance: true }).catch(() => ({ error: 'network' }));
  if (r.error) toast(`手番を終えられませんでした：${translateError(r.error)}`, 'error');
  else toast('命令なしで手番を終えました。');
  poll();
}
async function nextTurnClick() {
  if (S.view.over) { openDrawer('era'); return; }
  if (S.view.phase === 'lobby') { const r = await api.post('/api/start', {}); if (!r.ok) toast(r.error || '開幕できませんでした', 'error'); poll(); return; }
  const items = nextItems().filter(i => !i.none);
  if (items.length && !S.view.member?.ready) { S.nextIndex = 0; S.nextCurrent = items[0]; goNext(false); return; }
  if (isDirty()) await commit(true);
  const r = await api.post('/api/control', { advance: true });
  if (r.error) toast(`送信できませんでした：${r.error}`, 'error');
  poll();
}
/** Civ's notification stack: grouped, click to jump. */
function renderNotifs() {
  const items = nextItems().filter(i => !i.none);
  const groups = new Map();
  items.forEach((it, i) => {
    const g = it.drawer === 'diplomacy' ? '✉' : it.drawer === 'research' ? '✧' : it.unit !== undefined ? '⚑' : it.tile ? '⌂' : '•';
    if (!groups.has(g)) groups.set(g, { n: i, count: 0, title: it.title });
    groups.get(g).count++;
  });
  setHtml($('#notif-stack'), [...groups].map(([g, x]) => `<button class="notif" type="button" data-n="${x.n}">${g}${x.count > 1 ? `<span class="count">${x.count}</span>` : ''}<span class="tipx">${esc(x.title)}</span></button>`).join(''));
}

// ------------------------------------------------------------------ chain layer
function poolUsdc() {
  return S.view?.projection ? Number(S.view.projection.pool) / 1e6 : 0;
}
async function loadChain() {
  const g = S.view?.chain?.gateway; if (!g) return;
  if (!S.season) S.season = await fetch(`${g}/season`).then(r => r.json()).catch(() => null);
  const from = Math.max(0, S.view.tick - 14);
  const t = await fetch(`${g}/ticks?from=${from}`).then(r => r.json()).catch(() => null);
  if (t) S.ticks = t.records.slice(-14).reverse();
  renderPool(); renderRibbon(); if (S.chainOpen) renderChainDrawer();
}
function renderPool() {
  const p = poolUsdc();
  setHtml($('#pool-chip'), `<span class="k">賞金プール</span><span class="v">${fmt(p)}<small>USDC</small></span>`);
  $('#pool-chip').title = `参加費 ${usdc(S.view?.season?.entryFee)} USDC × 国民${(S.view?.members || []).length}人の80%、と市場の手数料・関税の80%${S.season ? `\nVault ${S.season.accounts?.vault}` : ''}`;
}
function renderChainBeat() {
  const v = S.view, el = $('#chain-beat'), c = v.chain;
  if (!c) { el.className = 'chain-beat local'; setHtml(el, `<span class="dot"></span><span class="l1">LOCAL</span><span class="l2">チェーンなし</span>`); return; }
  const t = c.lastTick;
  if (t && S.lastSealed !== t.tick) { S.lastSealed = t.tick; el.classList.remove('sealed'); void el.offsetWidth; el.classList.add('sealed'); loadChain(); }
  el.className = `chain-beat ${el.classList.contains('sealed') ? 'sealed' : ''}`;
  setHtml(el, `<span class="dot"></span><span class="l1">${t ? `T${t.tick} 封印 ✓` : '封印待ち'}</span><span class="l2">${c.layer === 'er' ? 'MagicBlock ER' : 'Solana'} · slot ${fmt(c.slot)}</span>`);
}
function toggleChain() { S.chainOpen = !S.chainOpen; $('#chain-drawer').hidden = !S.chainOpen; if (S.chainOpen) { renderChainDrawer(); loadChain(); } }
function renderChainDrawer() {
  const v = S.view, c = v.chain, el = $('#chain-drawer');
  if (!c) { setHtml(el, `<header><b>⛓ CHAIN</b><button class="x" data-close-chain>✕</button></header><div class="sec">ローカルモードです。エンジンはこのプロセスで動いています。<br><span class="k">--chain で起動すると MagicBlock ER 上のシーズンを表示します</span></div>`); return; }
  const er = c.endpoints?.er, x = sig => (er ? `https://explorer.solana.com/tx/${sig}?cluster=custom&customUrl=${encodeURIComponent(er)}` : '#');
  const reg = (S.season?.members || []).find(m => m.index === S.member);
  const ticks = (S.ticks || []).map((r, i) => `<div class="tk ${i === 0 ? 'new' : ''}"><span class="t">T${r.tick}</span><span><span class="m">${fmt(r.cu)} CU · ${r.submitted ?? '?'}/${4 * v.civs.length} offices · root ${r.root.slice(0, 8)}…</span><br><a href="${x(r.signature)}" target="_blank" rel="noopener">${short(r.signature)}</a></span><span class="ok">✓</span></div>`).join('');
  const gov = (v.chronicle || []).filter(e => /^(gov|recall|era)\|/.test(e.text)).slice(0, 6).map(e => `<div class="tk"><span class="t">T${e.tick}</span><span class="m">${esc(T.chronicleText(e.text)[1])}</span></div>`).join('');
  setHtml(el, `<header><b>⛓ CHAIN LENS</b><button class="x" data-close-chain>✕</button></header>
    <div class="sec"><span class="k">SEASON</span>${esc(c.seasonId)} · ${esc(c.cluster || 'localnet')}<span class="k">LAYER</span>${c.layer === 'er' ? 'MagicBlock Ephemeral Rollup' : 'Solana base'} · slot ${fmt(c.slot)}</div>
    <div class="sec"><span class="k">PRIZE VAULT</span>${fmt(poolUsdc())} USDC · ${short(c.accounts?.vault)}<span class="k">YOUR SESSION KEY</span>${reg ? `${short(reg.session)} · scope: orders & governance only · cannot move USDC` : '—'}</div>
    <div class="sec"><span class="k">GOVERNANCE ON CHAIN</span></div><div class="ticker">${gov || '<span class="k" style="padding:6px">選挙・解任・時代の記録はまだありません</span>'}</div>
    <div class="sec"><span class="k">SEALED TICKS</span></div><div class="ticker">${ticks || '<span class="k" style="padding:6px">読み込み中…</span>'}</div>
    <div class="sec"><span class="k">VERIFY IT YOURSELF</span><pre>cargo run --release --bin verify -- \\\n  --gateway ${esc(c.gateway)} --base ${esc(c.endpoints?.base)} --er ${esc(er)}</pre></div>`);
}
document.addEventListener('click', e => { if (e.target.closest('[data-close-chain]')) toggleChain(); });
setInterval(() => { if (S.view) renderNextTurn(); }, 500);
