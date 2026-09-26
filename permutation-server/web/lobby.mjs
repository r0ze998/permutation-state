// Registration (V5 §4), and the texts that describe who is in this season.
//
// Local mode: choose a nation, a name and the offices to stand for, and
// join (POST /api/join → a member token).
//
// Chain mode: every person joins with their own Solana wallet, exactly as
// an x402 agent does — 1) connect the wallet, 2) test USDC (/usdc, /faucet),
// 3) nation, a drawn name and 1–2 offices, 4) the cost and the deadline,
// 5) the free signature that makes the session key, 6) the x402 payment
// (the wallet signs Register; kind 2, votes NOBODY, the public deposit).
// The dialog then stays open through `registering` (countdown), `starting`
// and resolves at `playing` with the member id (the game is viewed as
// `?member=M`). A returning member whose key this browser keeps enters
// without any wallet popup; otherwise the wallet finds the member and signs
// once to recreate the key, which must equal the registered one (else: the
// key backup).
import * as T from './i18n.mjs';
import * as api from './api.mjs';
import * as chainio from './chainio.mjs';
import * as session from './session.mjs';
import * as wallet from './wallet.mjs';
import { $, html, setHtml, toast, usdc, short } from './util.mjs';
import { S, invalidate, aiRoster } from './state.mjs';
import { MAX_OFFICES, adoptRules, opsSharePct, poolSharePct } from './rules.mjs';
import { poll } from './sync.mjs';
import { memberName } from './sdk/codec.mjs';
import { randomBytes } from './sdk/bytes.mjs';
import { walletPicker, walletLine, connectWallet, reconnectSilently, disconnectWallet } from './connect.mjs';
import { findPastClaims, pastClaimsHtml, claimSeason } from './claim.mjs';
import { autoCommitSeconds } from './sealbook.mjs';
import { L, Lh, lang, lazyTable, langToggleHtml, onLangChange } from './lang.mjs';

let pending = null; // local mode: { info, resolve, refresh } while the dialog is open
let C = null;       // chain mode: the registration lobby while it is open (see chainLobby)
let described = false; // describeSeason has written the help texts (re-written on a language switch)

// A language switch: the lobby renders outside the render scheduler, and the
// help texts and the page title are written here, so both are written again.
onLangChange(() => {
  if (C?.shown) renderChain();
  else if (pending && $('#seats')?.open) { keepName(); renderLobby(); }
  if (described && S.view) describeSeason(S.view);
});

/** The language toggle, in the registration dialog's header (once). */
function mountToggle() {
  const dlg = $('#seats');
  if (!dlg || dlg.querySelector('[data-lang-toggle]')) return;
  dlg.insertAdjacentHTML('afterbegin', String(html`<div class="seats-lang" style="float:right;margin:-10px -8px 6px 12px">${langToggleHtml()}</div>`));
}

/** Local mode: show the member dialog; resolves with the new member token. */
export function pickMember(info) {
  const dlg = $('#seats');
  pending = { info, resolve: null, refresh: null };
  adoptRules(info);
  mountToggle();
  renderLobby();
  pending.refresh = setInterval(async () => {
    if (!dlg.open || !pending) return;
    const info = await api.tryGet('/api/lobby', pending.info);
    if (!pending) return;
    pending.info = info;
    adoptRules(info);
    renderLobby();
  }, 3000);
  dlg.oncancel = e => e.preventDefault(); // membership (or spectating) is required
  dlg.showModal();
  return new Promise(resolve => { pending.resolve = resolve; });
}

function renderLobby(error = '') {
  if (C) { renderChain(); return; }
  const info = pending.info;
  $('#seat-mode').textContent = L`ローカルのシーズン（テスト用USDC）`;
  $('#seat-nations').textContent = String(info.nations.length);
  const err = error ? html`<li class="seat-error">${error}</li>` : '';
  // Once the season has started nobody new can join: watch instead.
  if (info.phase !== 'lobby') {
    setHtml($('#seat-list'), html`<li class="seat-none">${L`シーズンは始まっていて、新しく参加することはできません。`}<a class="btn primary" href="spectate.html">${L`観戦する →`}</a></li>${err}`);
    return;
  }
  const pick = S.lobbyPick;
  const picked = pick.civ === null ? null : info.nations[pick.civ];
  setHtml($('#seat-list'), html`${info.nations.map(n => html`<li class="seat nation ${pick.civ === n.civ ? 'picked' : ''}" data-pick-civ="${n.civ}">
        <span class="swatch" style="background:${T.CIV_COLORS[n.civ]}"></span>
        <span class="who"><b>${T.civName(n.name)}</b><small>${L`国民 ${n.members}人`}${n.perMember ? ` · ${L`今の1人あたりの見込み ${usdc(n.perMember)} USDC`}` : ''}</small></span>
        <span class="meta">${pick.civ === n.civ ? L`選択中 ✓` : L`えらぶ`}</span></li>`)}<li class="seat-form"><label class="field">${L`名前`}<input id="join-name" maxlength="24" placeholder="${L`あなたの名前`}" value="${S.joinName}"></label>
         <div class="field">${L`立候補する役職（${MAX_OFFICES}つまで）`}<div class="row">${T.ROLES.map(r => html`<button type="button" class="btn ${pick.stand.includes(r) ? 'primary' : ''}" data-pick-stand="${r}">${T.ROLE_GLYPH[r]} ${T.ROLE_JA[r]}</button>`)}</div></div>
         <p class="desc">${L`参加費は全員同じ ${usdc(info.entryFee)} USDC（${poolSharePct(null)}%が賞金プール、${opsSharePct(null)}%が運営）。国民がいない役職はルールの代行が務めます。`}</p>
         <button class="btn primary wide" type="button" id="join-btn" ${picked ? '' : 'disabled'}>${picked ? L`${T.civName(picked.name)}の国民になる` : L`国を選んでください`}</button></li>${err}`);
}

const keepName = () => { S.joinName = $('#join-name')?.value ?? S.joinName; };
export function pickCiv(civ) { keepName(); S.lobbyPick.civ = civ; renderLobby(); }
export function pickStand(role) {
  keepName();
  const st = S.lobbyPick.stand;
  S.lobbyPick.stand = st.includes(role) ? st.filter(x => x !== role) : [...st, role].slice(-MAX_OFFICES);
  renderLobby();
}
export const join = () => finish(api.post('/api/join', { civ: S.lobbyPick.civ, name: $('#join-name').value, kind: 'human', stand: S.lobbyPick.stand }));

async function finish(request) {
  if (!pending) return;
  keepName();
  const res = await request;
  if (!pending) return;
  if (res.ok) {
    clearInterval(pending.refresh);
    api.tokenStore.set(res.token);
    $('#seats').close();
    const { resolve } = pending;
    pending = null;
    resolve(res.token);
    return;
  }
  const info = await api.tryGet('/api/lobby', pending.info);
  if (!pending) return;
  pending.info = info;
  renderLobby(api.translateError(res.error));
}

// ================================================================== chain mode
const STAGE = lazyTable({
  balance: () => L`残高を確認しています…`,
  quote: () => L`支払いの条件を確認しています…`,
  sign: () => L`ウォレットで参加費の支払いを承認してください`,
  send: () => L`送信しています…（確認まで15秒ほどかかることがあります）`,
});
const PHASE_TITLE = lazyTable({
  registering: () => L`ウォレットで国民になる`,
  starting: () => L`シーズンを準備しています`,
  playing: () => L`このシーズンに入る`,
});

const newName = () => memberName(randomBytes(32));
/** 1 or 2 offices at random, as the operator's AI members stand (the default says nothing about who chose it). */
function randomStand() {
  const [a, b] = randomBytes(2);
  const roles = [...T.ROLES];
  const first = roles.splice(a % roles.length, 1)[0];
  const picked = b & 1 ? [first] : [first, roles[(b >> 1) % roles.length]];
  return T.ROLES.filter(r => picked.includes(r));
}

/**
 * Chain mode: the registration lobby. Resolves with this browser's member
 * id once the season plays and the member is known (with its session key,
 * or view-only when the key cannot be recreated). Never resolves for a
 * visitor who is not a member (the dialog links to the spectator view).
 */
export function chainLobby(info) {
  return new Promise(resolve => {
    C = {
      info, resolve, season: null, reg: null, seasonError: '', fatal: '',
      usdc: null, name: newName(), session: null, // the key of a member-to-be (step 5)
      busy: '', error: '', notice: '', keyError: false, justJoined: false, viewOnly: false,
      past: null, pastBusy: '', pastLoading: null, pastRetryAt: 0, tried: false, polls: 0, refreshing: false, shown: false, timers: [], off: null, walletAddress: null, showBackup: false,
    };
    S.lobbyPick = { civ: null, stand: randomStand() };
    chainio.setGateway(info.gateway || info.chain?.gateway);
    wallet.discover();
    C.off = wallet.onChange(onWalletChange);
    C.timers.push(setInterval(pollChain, 3000), setInterval(tickClock, 1000));
    startChain();
  });
}

async function startChain() {
  const pre = await session.preflight();
  if (!C) return;
  if (!pre.ok) C.fatal = pre.error;
  await refresh(true);
  if (C && !C.shown) showChain();
}

function showChain() {
  C.shown = true;
  const dlg = $('#seats');
  dlg.oncancel = e => e.preventDefault(); // membership (or spectating) is required
  $('#seat-list').hidden = true;
  $('#seat-desc').hidden = true;
  $('#chain-lobby').hidden = false;
  mountToggle();
  renderChain();
  if (!dlg.open) dlg.showModal();
}

async function pollChain() {
  if (!C || C.refreshing) return;
  C.polls++;
  await refresh(C.polls % 3 === 0);
}

/** Re-read the play server's lobby (and the gateway's /season when `withSeason`); enter the game when it plays. */
async function refresh(withSeason) {
  if (!C) return;
  C.refreshing = true;
  try {
    const before = C.info.phase;
    const info = await api.tryGet('/api/lobby', C.info);
    if (!C) return;
    C.info = info;
    chainio.setGateway(info.gateway || info.chain?.gateway);
    if (withSeason || !C.season || info.phase !== before) await loadSeason();
    if (C && S.wallet && C.season && C.past === null && !C.pastLoading && Date.now() >= (C.pastRetryAt ?? 0)) loadPast();
  } finally {
    if (C) C.refreshing = false;
  }
  if (C && !maybeEnter()) renderChain();
}

async function loadSeason() {
  const s = await chainio.season();
  if (!C) return;
  if (!s.ok) { C.seasonError = T.errorText(s); return; }
  C.season = s;
  C.reg = chainio.registrationOf(s);
  C.seasonError = '';
  if (!pinSeason()) return;
  const cluster = chainio.pinned().cluster;
  await wallet.enableDevWallet({ cluster, allowed: chainio.devWalletAllowed(s) });
  if (!C.tried) {
    C.tried = true;
    await restoreMember();
    if (!S.session) await reconnectSilently(cluster);
    if (!C) return;
    if (S.wallet) { await afterWallet({ render: false }); return; }
  }
  matchMember();
}

/**
 * Pin the season from the play server's lobby (program, cluster, season,
 * entry fee; the gateway's /season when the lobby does not say yet), and
 * check that the gateway serves that very season.
 */
function pinSeason() {
  const info = C.info, s = C.season, ch = info.chain;
  try {
    const p = chainio.setPin({
      programId: ch?.programId ?? s.programId, cluster: ch?.cluster ?? s.cluster, seasonId: ch?.seasonId ?? s.season?.seasonId,
      entryFee: info.entryFee ?? s.season?.entryFee, accounts: ch?.accounts,
    });
    if (s.programId !== p.programId || String(s.season?.seasonId) !== p.seasonId || String(s.season?.entryFee) !== String(p.entryFee)) {
      C.fatal = T.CHAIN_ERROR_JA.PinMismatch;
      return false;
    }
    return true;
  } catch (e) {
    C.fatal = T.errorText(e);
    return false;
  }
}

/** A key this browser keeps for this season whose public key is a member's: that member, no wallet needed. */
async function restoreMember() {
  const members = C.season?.members || [];
  for (const ses of await session.cached(chainio.scope())) {
    const m = members.find(x => x.session === ses.publicKey);
    if (m) { S.session = ses; S.chainMember = m; return; }
  }
}

/** Keep S.chainMember in step with /season (and find it by the wallet when no key identified it). */
function matchMember() {
  const members = C?.season?.members || [];
  if (S.chainMember) {
    const m = members.find(x => x.index === S.chainMember.index);
    if (m) S.chainMember = m;
    return;
  }
  if (S.wallet) S.chainMember = members.find(x => x.wallet === S.wallet.address) ?? null;
}

/** The connected wallet changed (or was connected): find its member, its kept key, its balance and earlier prizes. */
async function afterWallet({ render = true } = {}) {
  if (!C) return;
  const address = S.wallet?.address ?? null;
  if (address === C.walletAddress) { if (render && !maybeEnter()) renderChain(); return; }
  C.walletAddress = address;
  C.error = ''; C.notice = ''; C.usdc = null; C.past = null; C.session = null; C.keyError = false;
  // A member found by a wallet (not by a kept key) follows the wallet.
  if (!S.session) S.chainMember = null;
  matchMember();
  const w = S.wallet;
  if (w && !S.session && C.season) {
    const ses = await session.restore(chainio.scope(), w.address);
    if (ses && S.chainMember && ses.publicKey === S.chainMember.session) S.session = ses;
    else if (ses && !S.chainMember) C.session = ses;
  }
  if (w && !S.chainMember) refreshUsdc();
  if (w) loadPast();
  if (render && !maybeEnter()) renderChain();
}

function onWalletChange() {
  if (C) afterWallet();
  else invalidate('drawer');
}

async function refreshUsdc() {
  const w = S.wallet?.address;
  if (!C || !w) return;
  const u = await chainio.usdc(w);
  if (!C || S.wallet?.address !== w) return;
  C.usdc = u.ok ? u : null;
  if (!u.ok) C.error = T.errorText(u);
  renderChain();
}

/** How long to wait before asking the gateway for earlier prizes again after it could not answer (ms). */
const PAST_RETRY_MS = 15_000;

/** The connected wallet's prizes in earlier seasons (claim.mjs findPastClaims, the gateway's /claims). */
async function loadPast() {
  const w = S.wallet?.address;
  if (!C || !w || C.pastLoading === w) return;
  C.pastLoading = w;
  const list = await findPastClaims(w).catch(() => null);
  if (!C) return;
  if (C.pastLoading === w) C.pastLoading = null;
  if (S.wallet?.address !== w) return;
  if (list === null) { C.pastRetryAt = Date.now() + PAST_RETRY_MS; return; } // asked again by a later refresh
  C.past = list;
  renderChain();
}

function maybeEnter() {
  if (!C || C.info.phase !== 'playing' || !S.chainMember || !(S.session || C.viewOnly)) return false;
  const { resolve, timers, off } = C;
  for (const t of timers) clearInterval(t);
  off?.();
  C = null;
  const dlg = $('#seats');
  if (dlg.open) dlg.close();
  resolve(S.chainMember.index);
  return true;
}

function busy(text) { if (C) { C.busy = text || ''; renderChain(); } }

// ------------------------------------------------------------------ chain lobby: actions (input.mjs)
/** A wallet button (lobby or claim card). */
export async function connectNamed(name) {
  const cluster = chainio.pinned()?.cluster;
  if (!cluster) return;
  if (C) { C.error = ''; busy(L`ウォレットの接続を待っています…`); }
  try {
    await connectWallet(name, cluster);
  } catch (e) {
    if (C) C.error = T.errorText(e); else toast(T.errorText(e), 'error');
  }
  if (C) { C.busy = ''; await afterWallet(); }
  invalidate('drawer');
}

export async function walletDisconnect() {
  await disconnectWallet();
  if (C) await afterWallet();
  invalidate('drawer');
}

/** The lobby's buttons (`data-reg`). */
export function regAction(what) {
  if (!C) return;
  const run = {
    faucet, balance: refreshUsdc, derive, join: joinChain, import: () => importKey($('#reg-import')?.value),
    reroll: () => { C.name = newName(); renderChain(); },
    'backup-copy': backupCopy, 'backup-save': backupSave,
    view: () => { C.viewOnly = true; maybeEnter(); },
  }[what];
  run?.();
}

async function faucet() {
  const w = S.wallet;
  if (!w || C.busy) return;
  C.error = ''; C.notice = '';
  busy(L`テスト USDC を受け取っています…`);
  const r = await chainio.faucet(w.address);
  if (!C) return;
  C.busy = '';
  if (r.ok) C.notice = Number(r.amount) > 0 ? L`テスト USDC ${usdc(r.amount)} を受け取りました（価値のないテスト用トークンです）。` : L`このウォレットは、このシーズンのテスト USDC を受け取り済みです。`;
  else C.error = T.errorText(r);
  await refreshUsdc();
}

async function derive() {
  const w = S.wallet;
  if (!w || C.busy) return;
  C.error = ''; C.notice = '';
  const pre = await session.preflight();
  if (!pre.ok) { C.error = pre.error; renderChain(); return; }
  busy(L`ウォレットでメッセージに署名してください（無料・取引ではありません）`);
  try {
    const ses = await session.derive({ wallet: w, ...chainio.scope() });
    if (!C) return;
    if (S.chainMember) {
      // A member recreating its key: it must be the registered one.
      if (ses.publicKey === S.chainMember.session) { session.remember(ses); S.session = ses; C.keyError = false; }
      else C.keyError = true;
    } else {
      // A member-to-be: nobody may hold this key already (checked on a fresh /season).
      await loadSeason();
      if (!C) return;
      const members = C.season?.members || [];
      if (S.chainMember) { if (ses.publicKey === S.chainMember.session) { session.remember(ses); S.session = ses; } else C.keyError = true; }
      else if (members.some(x => x.session === ses.publicKey)) C.error = T.CHAIN_ERROR_JA.SessionInUse;
      else { session.remember(ses); C.session = ses; }
    }
  } catch (e) {
    if (C) C.error = T.errorText(e);
  }
  if (!C) return;
  C.busy = '';
  if (!maybeEnter()) renderChain();
}

async function joinChain() {
  const w = S.wallet, ses = C.session;
  if (!w || !ses || C.busy || S.lobbyPick.civ === null || !S.lobbyPick.stand.length) return;
  C.error = ''; C.notice = '';
  const r = await chainio.join({ wallet: w, session: ses, civ: S.lobbyPick.civ, name: C.name, stand: S.lobbyPick.stand, onStep: st => busy(STAGE[st]) });
  if (!C) return;
  C.busy = '';
  if (r.ok) {
    S.session = ses;
    C.session = null;
    S.chainMember = { index: r.member, civ: r.civ, name: r.name ?? C.name, wallet: w.address, session: ses.publicKey };
    C.justJoined = true;
    await refresh(true);
    return;
  }
  C.error = r.retry ? T.CHAIN_ERROR_JA.BlockhashExpired : T.errorText(r);
  if (r.code === 'InsufficientFunds') await refreshUsdc();
  else if (r.code === 'AlreadyMember' || r.code === 'AlreadyInitialized' || r.code === 'SessionInUse') await refresh(true);
  else renderChain();
}

async function importKey(text) {
  if (!C) return;
  C.error = ''; C.notice = '';
  try {
    if (!C.season) await loadSeason();
    const { session: ses, member } = await session.importBackup(text, chainio.scope(), C.season?.members || []);
    S.session = ses; S.chainMember = member; C.keyError = false;
    C.notice = L`鍵を読み込みました。`;
  } catch (e) {
    C.error = T.errorText(e);
  }
  if (!maybeEnter()) renderChain();
}
/** A key backup file chosen in the lobby. */
export async function importKeyFile(file) {
  if (!file || !C) return;
  await importKey(await file.text().catch(() => ''));
}

async function backupCopy() {
  if (!S.session) return;
  try {
    await navigator.clipboard.writeText(S.session.backupText());
    C.notice = L`鍵をコピーしました。パスワードマネージャーなど安全な場所に保存してください。`;
    C.showBackup = false;
  } catch {
    C.showBackup = true; // copy by hand from the box
    C.notice = L`コピーできませんでした。下の内容を選んで、安全な場所に保存してください。`;
  }
  renderChain();
}

function backupSave() {
  if (!S.session) return;
  const url = URL.createObjectURL(new Blob([S.session.backupText()], { type: 'text/plain' }));
  const a = document.createElement('a');
  a.href = url;
  a.download = `permutation-state-season${S.session.scope.seasonId}-key.txt`;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 2000);
  C.notice = L`鍵をファイルに保存しました。安全な場所に保管してください。`;
  renderChain();
}

/** An earlier season's prize, to the connected wallet (`data-claim-season`). */
export async function claimPastSeason(seasonId) {
  if (!C || C.pastBusy) return;
  C.error = ''; C.notice = '';
  C.pastBusy = '…';
  renderChain();
  const r = await claimSeason(seasonId, text => { if (C) { C.pastBusy = text || '…'; renderChain(); } });
  if (!C) return;
  C.pastBusy = '';
  if (r.ok) {
    C.notice = L`シーズン ${seasonId} の賞金を受け取りました。`;
    C.past = (C.past || []).map(x => (x.seasonId === String(seasonId) ? { ...x, done: true } : x));
  } else if (r.code === 'NoSuchMember' || r.code === 'NotInitialized' || r.code === 'WrongPda') {
    C.error = L`このウォレットは、シーズン ${seasonId} の国民ではありませんでした。`;
    C.past = (C.past || []).filter(x => x.seasonId !== String(seasonId));
  } else C.error = T.errorText(r);
  renderChain();
}

// ------------------------------------------------------------------ chain lobby: rendering
const pad = n => String(n).padStart(2, '0');
const clockText = msLeft => {
  const s = Math.max(0, Math.ceil(msLeft / 1000));
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
  return h ? `${h}:${pad(m)}:${pad(s % 60)}` : `${m}:${pad(s % 60)}`;
};
const hhmm = t => { const d = new Date(t); return `${pad(d.getHours())}:${pad(d.getMinutes())}`; };
const serverNow = () => Date.now() + (C?.reg?.offsetMs ?? 0);

/** The countdowns (`data-countdown`), every second: text only, so the form is never re-rendered by the clock. */
function tickClock() {
  if (!C?.reg?.closesAt) return;
  const left = C.reg.closesAt - serverNow();
  for (const el of document.querySelectorAll('#chain-lobby [data-countdown]')) el.textContent = left > 0 ? clockText(left) : L`締切を過ぎました`;
}

const registrationOpen = () => C.info.phase === 'registering' && (!C.reg?.closesAt || C.reg.closesAt > serverNow());
const nationNames = () => C.season?.nations || [];
const nationName = civ => T.civName(nationNames()[civ] ?? L`国${civ}`);
const deadlineText = () => (C.reg?.closesAt
  ? Lh`締切 <b>${hhmm(C.reg.closesAt)}</b>（あと <b class="num" data-countdown></b>）に、人数に関わらずシーズンが始まります`
  : L`必要な人数がそろうとシーズンが始まります（テスト用の設定）`);

function renderChain() {
  if (!C) return;
  const el = $('#chain-lobby');
  const phase = C.info.phase;
  $('#seat-title').textContent = S.chainMember ? (phase === 'registering' ? L`登録しました` : PHASE_TITLE[phase] ?? PHASE_TITLE.registering)
    : C.season && !C.fatal && !registrationOpen() ? L`登録は締め切られました` : PHASE_TITLE.registering;
  // Keep what the user typed into the import box across re-renders.
  const typed = $('#reg-import')?.value ?? '';
  const detailsOpen = $('#reg-import')?.closest('details')?.open ?? false;
  const s = C.season;
  let body;
  if (C.fatal) body = html`<p class="seat-error">${C.fatal}</p>`;
  else if (!s) body = html`<p class="desc"><span class="spinner"></span>${L`ゲートウェイからシーズンを読み込んでいます…`}${C.seasonError ? html`<br><span class="seat-error">${C.seasonError}</span>` : ''}</p>`;
  else if (S.chainMember) body = memberBox(phase);
  else if (registrationOpen()) body = steps();
  else body = closedBox(phase);
  const notes = html`${C.busy ? html`<p class="reg-busy"><span class="spinner"></span>${C.busy}</p>` : ''}${C.error ? html`<p class="seat-error">${C.error}</p>` : ''}${C.notice ? html`<p class="reg-notice">${C.notice}</p>` : ''}`;
  const past = s && S.wallet && !C.fatal ? pastClaimsHtml(C.past, C.pastBusy) : '';
  setHtml(el, html`${intro(phase)}${body}${notes}${past}${C.pastBusy && C.pastBusy !== '…' ? html`<p class="reg-busy"><span class="spinner"></span>${C.pastBusy}</p>` : ''}`);
  const box = $('#reg-import');
  if (box && typed && !box.value) box.value = typed;
  if (box && detailsOpen) box.closest('details').open = true;
  tickClock();
}

function intro(phase) {
  if (phase !== 'registering' || S.chainMember) return '';
  return html`<p class="desc">${L`国は${nationNames().length || '—'}つ。自分の Solana ウォレットで参加費（USDC）を払って国民になります。手数料（SOL）は運営が払うので、ウォレットに SOL は要りません。`}</p>`;
}

function importBox(open) {
  return html`<details class="reg-import" ${open ? 'open' : ''}><summary>${L`鍵のバックアップを読み込む`}</summary>
    <input id="reg-import" type="text" autocomplete="off" spellcheck="false" placeholder="${L`鍵（16進64文字）またはバックアップの内容`}">
    <div class="row"><button class="btn" type="button" data-reg="import">${L`読み込む`}</button><label class="btn">${L`ファイルを選ぶ`}<input type="file" id="reg-import-file" accept=".txt,text/plain" hidden></label></div></details>`;
}

function backupBox() {
  const ses = S.session;
  return html`<div class="reg-backup ${C.justJoined ? 'fresh' : ''}"><b>${L`鍵をバックアップ`}</b>
    <p class="desc">${ses.stored
    ? L`ゲーム内の鍵はこのブラウザに保存されています。別の端末では、同じウォレットで署名すれば同じ鍵が作られます。ウォレットが同じ鍵を作れないときのために、バックアップを保存してください。`
    : Lh`ゲーム内の鍵はこのブラウザに<b>保存できませんでした</b>（再読み込みすると、もう一度署名が必要です）。別の端末では、同じウォレットで署名すれば同じ鍵が作られます。ウォレットが同じ鍵を作れないときのために、バックアップを保存してください。`}</p>
    <div class="row"><button class="btn" type="button" data-reg="backup-copy">${L`鍵をコピー`}</button><button class="btn" type="button" data-reg="backup-save">${L`ファイルに保存`}</button></div>
    ${C.showBackup ? html`<textarea class="reg-backup-text" readonly rows="4">${ses.backupText()}</textarea>` : ''}</div>`;
}

function memberBox(phase) {
  const m = S.chainMember;
  const head = html`<div class="reg-head"><span class="swatch" style="background:${T.CIV_COLORS[m.civ]}"></span><span class="who"><b>${m.name}</b><small>${L`${nationName(m.civ)}の国民（#${m.index}）`}</small></span><span class="tag positive">${L`登録済み ✓`}</span></div>`;
  let key;
  if (S.session) key = html`<p class="desc">${L`ゲーム内の鍵 ✓`} <code>${short(S.session.publicKey, 6, 6)}</code></p>`;
  else if (C.keyError) {
    key = html`<p class="seat-error">${L`このウォレットの署名から作った鍵が、登録されている鍵と一致しません（ウォレットによっては同じ署名を再現できません）。鍵のバックアップを読み込んでください。観戦と賞金の受け取りはこのままできます。`}</p>${importBox(true)}`;
  } else if (S.wallet && S.wallet.address === m.wallet) {
    key = html`<p class="desc">${L`このブラウザには、この国民のゲーム内の鍵がありません。ウォレットでもう一度署名すると、登録時と同じ鍵を作り直します（無料・取引ではありません）。`}</p>
      <button class="btn primary" type="button" data-reg="derive" ${C.busy ? 'disabled' : ''}>${L`署名して鍵を作り直す`}</button>${importBox(false)}`;
  } else {
    key = html`<p class="desc">${L`このブラウザには、この国民のゲーム内の鍵がありません。登録したウォレット（${short(m.wallet, 4, 4)}）を接続して署名するか、鍵のバックアップを読み込んでください。`}</p>${S.wallet ? html`<button class="btn" type="button" data-wallet-disconnect>${L`別のウォレットに切り替える`}</button>` : walletPicker(chainio.pinned()?.cluster)}${importBox(false)}`;
  }
  let now;
  if (phase === 'registering') now = html`<p class="desc">${Lh`${deadlineText()}。このページは閉じても大丈夫です（同じブラウザか、同じウォレットで戻れます）。`}</p>`;
  else if (phase === 'starting') now = html`<p class="desc"><span class="spinner"></span>${L`シーズンを準備しています…（世界の生成と着任。数分かかることがあります）`}</p>`;
  else now = S.session ? html`<p class="desc"><span class="spinner"></span>${L`入場しています…`}</p>`
    : html`<p class="desc">${L`鍵がなくても、自分の国を見ることはできます（命令・投票・会話はできません）。`}</p><button class="btn" type="button" data-reg="view">${L`鍵なしで見る`}</button>`;
  return html`<div class="reg-done">${head}${key}${now}${S.session ? backupBox() : ''}</div>`;
}

function steps() {
  const p = chainio.pinned(), w = S.wallet;
  const need = p.entryFee + (C.reg?.deposit ?? 0n);
  const bal = C.usdc ? chainio.bestBalance(C.usdc) : null;
  const funded = bal !== null && bal >= need;
  const pick = S.lobbyPick;
  const chosen = pick.civ !== null && pick.stand.length >= 1;
  const keyed = !!C.session;
  const state = (done, ready) => (done ? 'done' : ready ? 'active' : 'todo');

  const s1 = w ? html`<p class="desc">${walletLine(w)} <button class="btn small" type="button" data-wallet-disconnect>${L`切り替える`}</button></p>`
    : html`${walletPicker(p.cluster)}<p class="desc">${L`登録済みの方は、登録したウォレットを接続すると自動で見つかります（この端末にそのウォレットがなければ、鍵のバックアップを読み込めます）。`}</p>${importBox(false)}`;
  let s2;
  if (!w) s2 = html`<p class="desc">${L`ウォレットを接続すると、残高を確かめます。`}</p>`;
  else if (!C.usdc) s2 = html`<p class="desc"><span class="spinner"></span>${L`残高を確認しています…`}</p>`;
  else {
    const faucetBtn = p.cluster === 'mainnet' ? html`<p class="desc">${L`このウォレットに USDC を入金してから「残高を更新」を押してください。`}</p>`
      : html`<button class="btn usdc" type="button" data-reg="faucet" ${C.busy ? 'disabled' : ''}>${L`テスト USDC を受け取る（無料）`}</button>`;
    s2 = html`<p class="desc">${Lh`残高 <b>${usdc(bal)} USDC</b>（必要 ${usdc(need)} USDC）`}${funded ? ' ✓' : ''}</p>
      <div class="row">${funded ? '' : faucetBtn}<button class="btn small" type="button" data-reg="balance">${L`残高を更新`}</button></div>
      ${p.cluster === 'mainnet' ? '' : html`<p class="desc">${L`テスト USDC は運営のテスト用トークンです（価値はありません）。`}</p>`}`;
  }
  const counts = C.season?.season?.nationMembers || [];
  const nations = nationNames().map((n, civ) => html`<button type="button" class="reg-nation ${pick.civ === civ ? 'picked' : ''}" data-pick-civ="${civ}">
      <span class="swatch" style="background:${T.CIV_COLORS[civ]}"></span><b>${T.civName(n)}</b><small>${L`国民 ${counts[civ] ?? (C.season.members || []).filter(m => m.civ === civ).length}人`}</small></button>`);
  const s3 = html`<div class="reg-nations">${nations}</div>
    <div class="field">${L`名前（全員が同じ方法でランダムに選びます）`}<div class="row"><b class="reg-name">${C.name}</b><button class="btn small" type="button" data-reg="reroll" title="${L`引き直す`}">🎲 ${L`引き直す`}</button></div></div>
    <div class="field">${L`立候補する役職（1〜${MAX_OFFICES}つ。第1回の選挙は立候補者からランダムに決まります）`}<div class="row">${T.ROLES.map(r => html`<button type="button" class="btn small ${pick.stand.includes(r) ? 'primary' : ''}" data-pick-stand="${r}">${T.ROLE_GLYPH[r]} ${T.ROLE_JA[r]}</button>`)}</div></div>`;
  const dep = C.reg?.deposit ?? 0n;
  const s4 = html`<p class="desc">${Lh`参加費 <b>${usdc(p.entryFee)} USDC</b>${dep ? L` ＋ 国庫への預け入れ ${usdc(dep)} USDC` : ''}（参加費の${poolSharePct(null)}%が賞金プール、${opsSharePct(null)}%が運営）。`}</p>
    <p class="desc"><span class="tag bad">${L`参加費は返金されません`}</span></p>
    <p class="desc">${Lh`${deadlineText()}。いま ${C.reg?.members ?? (C.season.members || []).length} 人が登録しています。`}</p>`;
  const s5 = keyed ? html`<p class="desc">${L`✓ ゲーム内の鍵`} <code>${short(C.session.publicKey, 6, 6)}</code></p>`
    : html`<p class="desc">${Lh`ウォレットでメッセージに署名します（<b>無料</b>・取引ではありません）。この署名から、このシーズン専用の「ゲーム内の鍵」をこのブラウザで作ります。鍵は命令・投票・会話の署名に使います。賞金の受け取りやウォレットのトークンの移動はできませんが、役職者になると国の国庫（市場・契約）は動かせます。`}</p>
      <button class="btn primary" type="button" data-reg="derive" ${w && !C.busy ? '' : 'disabled'}>${L`署名して鍵を作る`}</button>`;
  const ready = w && funded && chosen && keyed && !C.busy;
  const s6 = html`<button class="btn primary wide" type="button" data-reg="join" ${ready ? '' : 'disabled'}>${L`参加費を払って参加（${usdc(need)} USDC）`}</button>
    <p class="desc">${L`ウォレットに ${usdc(need)} USDC を支払う取引の承認を求めます。手数料（SOL）は運営が払います。${pick.civ === null ? L`国を選んでください。` : ''}`}</p>`;
  return html`<ol class="reg-steps">
    <li class="${state(!!w, true)}"><span><b>${L`ウォレットを接続`}</b>${s1}</span></li>
    <li class="${state(funded, !!w)}"><span><b>${L`USDC の残高`}</b>${s2}</span></li>
    <li class="${state(chosen, true)}"><span><b>${L`国・名前・立候補する役職`}</b>${s3}</span></li>
    <li class="active"><span><b>${L`参加費と開始`}</b>${s4}</span></li>
    <li class="${state(keyed, !!w)}"><span><b>${L`署名して鍵を作る`}</b>${s5}</span></li>
    <li class="${state(false, ready)}"><span><b>${L`参加費を払って参加`}</b>${s6}</span></li></ol>`;
}

function closedBox(phase) {
  const why = phase === 'registering' ? L`登録は締め切られました。まもなくシーズンが始まります。`
    : phase === 'starting' ? L`登録は締め切られました。シーズンを準備しています…` : L`このシーズンの登録は締め切られました。`;
  const who = S.wallet
    ? html`<p class="desc">${L`このウォレット（${short(S.wallet.address, 4, 4)}）は、このシーズンの国民ではありません。`}</p><button class="btn" type="button" data-wallet-disconnect>${L`別のウォレットに切り替える`}</button>`
    : html`<p class="desc">${L`このシーズンの国民の方は、登録したウォレットを接続するか、鍵のバックアップを読み込んでください。`}</p>${walletPicker(chainio.pinned()?.cluster)}${importBox(false)}`;
  return html`<div class="seat-none">${why}<a class="btn primary" href="spectate.html">${L`観戦する →`}</a></div>${who}`;
}

// ================================================================== who is in this season
/** Who else is in this season, from the game view's member list and AI roster (`aiRoster`). */
export function othersText(v) {
  const ms = v?.members || [];
  const ai = Number(aiRoster(v).aiCount) || 0;
  return L`この季節の国民は${ms.length}人。${ai ? L`うち${ai}人は運営のAI国民です（誰かは、住む都市が落ちたときとシーズンの終わりに公開）。` : ''}国民のいない役職はルールの代行が務めます。`;
}

/** Help texts and titles that match this season (nations, members, clock, terms). */
export function describeSeason(v) {
  described = true;
  const nations = v.civs.length;
  document.title = L`PERMUTATION STATE — ${nations}つの国、ひとつの世界`;
  $('#help-title').textContent = L`${nations}つの国のひとつの国民として、国を動かす。`;
  $('#help-desc').textContent = L`${nations}つの国が同じ地図を共有しています。${othersText(v)}${v.tickSeconds}秒ごとの「ティック」で、全ての国の命令が同時に解決されます。`;
  for (const el of document.querySelectorAll('[data-season]')) {
    const x = v.season?.[el.dataset.season];
    if (x !== undefined && x !== null) el.textContent = String(x);
  }
  if (v.chain) {
    const auto = Math.round(autoCommitSeconds(v.tickSeconds));
    // Sentences side by side: no space between them in Japanese, one in English.
    $('#help-mode').textContent = [
      L`このシーズンはオンチェーンです（MagicBlock ER）。時計は止まりません。`,
      L`「確定する」で各役職の命令を封印し、中身を運営のゲートウェイに預けてから、封印（ハッシュ）をこのブラウザのゲーム内の鍵で署名してチェーンに送ります。締切のあと数秒でゲートウェイが公開し、全ての国を同時に解決します。`,
      L`締切の約${auto}秒前には下書きを自動で確定します（このタブを表示している間だけ）。「手番を終える」は、まだ何も送っていない役職に空の封印を送ります。`,
      L`投票・立候補・献策・支持・リコール・会話も同じ鍵で署名して送ります（締切後の数秒間に出したものは次のティックに送ります）。手数料は運営が払います。`,
      L`ウォレットが署名するのは参加費の支払いと賞金の受け取りだけです。`,
    ].join(lang() === 'en' ? ' ' : '');
    const start = $('#help [data-start]');
    if (start) start.textContent = L`閉じてはじめる →`;
  }
}

/** Start the season (local lobby) or resume the clock (local); on chain the clock never stops. */
export async function startSeason() {
  if (S.view?.chain) return;
  if (S.view?.phase === 'lobby') {
    const r = await api.post('/api/start', {});
    if (!r.ok) toast(r.error || L`開幕できませんでした`, 'error');
  } else await api.post('/api/control', { paused: false });
  poll();
}
