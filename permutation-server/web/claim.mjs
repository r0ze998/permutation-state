// The season's result and the prize claim (chain mode). "あなたの賞金" is
// the program's own amount (codec.mjs claimParts: prize plus the member's
// share of what is left in its nation's treasury) from a fresh /season.
// Before the season is finalized (up to about an hour after the last tick,
// while the operator reveals its AI roster) the card says so; then "受け取る"
// has the wallet sign Claim to its own token account, and the gateway pays
// the fee. Nothing is shown when there is nothing to claim.
//
// Also the lobby's "前のシーズンの賞金を受け取る": the connected wallet's
// unclaimed members in the seasons this one follows, as the gateway's
// GET /claims reads them (this page never calls an RPC itself).
import * as T from './i18n.mjs';
import * as chainio from './chainio.mjs';
import { html, usdc, short, toast } from './util.mjs';
import { S, invalidate } from './state.mjs';
import { walletPicker, walletLine } from './connect.mjs';
import { L, lazyTable } from './lang.mjs';

const REFRESH_MS = 20_000;
/** After a failed load. */
const RETRY_MS = 3_000;
const STAGE = lazyTable({
  status: () => L`精算の状況を確認しています…`,
  sign: () => L`ウォレットで受け取りを承認してください（手数料は運営が払います）`,
  send: () => L`送信しています…（確認まで15秒ほどかかることがあります）`,
});

// Claim signatures sent from this browser, for the explorer link after a reload.
const sigKey = (seasonId, w) => { const p = chainio.pinned(); return `ps-claim:${p?.cluster}:${p?.programId}:${seasonId}:${w}`; };
const sigGet = (seasonId, w) => { try { return localStorage.getItem(sigKey(seasonId, w)); } catch { return null; } };
const sigSet = (seasonId, w, sig) => { try { localStorage.setItem(sigKey(seasonId, w), sig); } catch { /* unavailable */ } };

/** Time to fetch again: every 20 s once loaded, every 3 s until then. */
const due = c => performance.now() - c.at >= (c.season ? REFRESH_MS : RETRY_MS);
// `at` starts at -Infinity: never loaded, so the first render fetches at once.
const card = () => (S.claim ??= { at: -Infinity, loading: false, season: null, member: null, parts: null, busy: '', error: '' });

/** Fetch /season again (at most every 20 s unless `force`); re-renders the drawer. */
export async function refreshClaim(force = false) {
  const c = card();
  if (c.loading || (!force && !due(c)) || !S.chainMember) return;
  c.loading = true;
  try {
    const s = await chainio.season();
    if (s.ok) {
      c.season = s;
      c.member = (s.members || []).find(m => m.index === S.chainMember.index) ?? null;
      c.parts = c.member ? chainio.claimOf(s, c.member) : null;
    }
  } finally {
    c.at = performance.now();
    c.loading = false;
    invalidate('drawer');
    // Nothing loaded (no gateway yet, rate limit, network): try again soon
    // rather than waiting for the drawer to re-render.
    if (!c.season) setTimeout(() => refreshClaim(true), RETRY_MS);
  }
}

const fmtUsdc = x => usdc(Number(x ?? 0n));

/**
 * The result card for the season-end drawers, or '' (not chain mode, not a
 * member, or nothing to claim).
 */
export function claimCard(v) {
  if (!v?.chain || !S.chainMember) return '';
  const c = card();
  if (due(c)) refreshClaim();
  const s = c.season, m = c.member;
  if (!s || !m) return html`<div class="claim-card"><div class="eyebrow">${L`RESULT · あなたの賞金`}</div><p class="desc">${L`結果を読み込んでいます…`}</p></div>`;
  const status = s.season?.status;
  const busy = c.busy ? html`<p class="claim-busy"><span class="spinner"></span>${c.busy}</p>` : '';
  const err = c.error ? html`<p class="seat-error">${c.error}</p>` : '';
  if (status !== 'Finalized') {
    const projected = Number(v.member?.projectedPayout ?? 0);
    if (!projected && !(Number(m.shares ?? 0) > 0)) return '';
    return html`<div class="claim-card"><div class="eyebrow">${L`RESULT · あなたの賞金`}</div>
      <h3>${L`見込み ${usdc(projected)} USDC`}</h3>
      <p class="desc"><span class="tag warning">${L`精算待ち（最長約1時間）`}</span> ${L`シーズンの精算（運営のAIメンバーの公開と配分の確定）が終わると、ここから受け取れます。`}</p></div>`;
  }
  const total = c.parts?.total ?? 0n;
  if (total <= 0n) return '';
  const head = html`<div class="eyebrow">${L`RESULT · あなたの賞金`}</div><h3>${L`あなたの賞金 ${fmtUsdc(total)} USDC`}</h3>
    <p class="desc">${L`賞金 ${fmtUsdc(c.parts.prize)} USDC${c.parts.refund ? L` ＋ 勢力の資金の残りの返還 ${fmtUsdc(c.parts.refund)} USDC` : ''}。登録したウォレット（${short(m.wallet, 4, 4)}）のトークン口座に届きます。`}</p>`;
  if (m.claimed) {
    const sig = sigGet(chainio.pinned()?.seasonId, m.wallet);
    const link = sig && chainio.explorerTx(sig, { base: s.endpoints?.base ?? v.chain?.endpoints?.base });
    return html`<div class="claim-card done">${head}<p class="desc"><span class="tag positive">${L`受け取り済み ✓`}</span>${link ? html` <a href="${link}" target="_blank" rel="noopener">${L`エクスプローラーで見る →`}</a>` : ''}</p></div>`;
  }
  let action;
  if (!S.wallet) action = html`<p class="desc">${L`受け取るには、登録したウォレットを接続してください。`}</p>${walletPicker(chainio.pinned()?.cluster)}`;
  else if (S.wallet.address !== m.wallet) action = html`<p class="seat-error">${L`接続中のウォレット（${short(S.wallet.address, 4, 4)}）は登録したものと違います。${short(m.wallet, 4, 4)} に切り替えてください。`}</p><button class="btn" type="button" data-wallet-disconnect>${L`接続を切る`}</button>`;
  else action = html`<p class="desc">${walletLine(S.wallet)}</p><button class="btn primary" type="button" data-claim-prize ${c.busy ? 'disabled' : ''}>${L`受け取る（${fmtUsdc(total)} USDC）`}</button>`;
  return html`<div class="claim-card">${head}${action}${busy}${err}</div>`;
}

/** "受け取る" (Claim): the wallet signs Claim for this season. */
export async function claimPrize() {
  const c = card();
  if (c.busy || !S.wallet || !c.member) return;
  c.error = '';
  const r = await chainio.claim({ wallet: S.wallet, onStep: st => { c.busy = STAGE[st] ?? ''; invalidate('drawer'); } });
  c.busy = '';
  if (r.ok) {
    sigSet(chainio.pinned()?.seasonId, S.wallet.address, r.signature);
    toast(L`賞金を受け取りました（${short(r.dest, 4, 4)}）`, 'good');
  } else c.error = T.errorText(r);
  await refreshClaim(true);
}

// ------------------------------------------------------------------ earlier seasons (lobby)
const amountOf = x => { try { return BigInt(String(x ?? '')); } catch { return null; } };

/**
 * The connected wallet's unclaimed prizes in the seasons this one follows:
 * `[{seasonId, total (bigint) | null when it could not be read}]`, from the
 * gateway's GET /claims (finalized, not claimed, more than 0). A gateway
 * older than /claims (404 NotFound): every season of its /history lineage
 * with an unknown amount (claiming then tells). Resolves null when the
 * gateway could not answer now (ask again later).
 */
export async function findPastClaims(walletAddress) {
  const current = chainio.pinned()?.seasonId;
  const r = await chainio.claims(walletAddress);
  if (r.ok && Array.isArray(r.claims)) {
    const out = [], seen = new Set();
    for (const c of r.claims) {
      const seasonId = String(c?.seasonId ?? ''), total = amountOf(c?.amount);
      if (!seasonId || seasonId === current || seen.has(seasonId)) continue;
      if (c.claimed || c.status !== 'Finalized' || total === null || total <= 0n) continue;
      seen.add(seasonId);
      out.push({ seasonId, total });
    }
    return out;
  }
  if (r.httpStatus !== 404 || r.code !== 'NotFound') return null;
  const h = await chainio.history();
  if (!h.ok) return null;
  const ids = [...new Set((h.lineage || []).map(x => String(x.seasonId)))].filter(id => id !== current);
  return ids.map(seasonId => ({ seasonId, total: null }));
}

/**
 * Markup for the lobby: one row per earlier season with a prize to claim;
 * seasons whose member could not be read are folded away (claiming tells).
 */
export function pastClaimsHtml(list, busy) {
  if (!list?.length) return '';
  const row = x => html`<div class="list-row" style="cursor:default"><div class="main"><div class="title">${L`シーズン ${x.seasonId}`}</div>
      <div class="meta">${x.done ? L`受け取り済み ✓` : x.total === null ? L`金額をゲートウェイから読めませんでした` : `${fmtUsdc(x.total)} USDC`}</div></div>
      ${x.done ? '' : html`<button class="btn primary" type="button" data-claim-season="${x.seasonId}" ${busy ? 'disabled' : ''}>${L`受け取る`}</button>`}</div>`;
  const known = list.filter(x => x.total !== null), unknown = list.filter(x => x.total === null);
  return html`<div class="past-claims">${known.length ? html`<div class="section-title">${L`前のシーズンの賞金を受け取る`}</div>${known.map(row)}` : ''}
    ${unknown.length ? html`<details><summary class="section-title">${L`前のシーズンの賞金を確かめる（${unknown.length}シーズン）`}</summary>
      <p class="desc">${L`このウォレットがそのシーズンのメンバーだったかを、ゲートウェイから読めませんでした。受け取りを試すとわかります（ウォレットの署名が必要です。メンバーでなかったシーズンでは受け取れません）。`}</p>${unknown.map(row)}</details>` : ''}</div>`;
}

/** Claim an earlier season's prize with the connected wallet; returns the chainio result. */
export async function claimSeason(seasonId, onStep) {
  if (!S.wallet) return chainio.fail('NoWallet');
  const r = await chainio.claim({ wallet: S.wallet, seasonId, onStep: st => onStep?.(STAGE[st] ?? '') });
  if (r.ok) sigSet(seasonId, S.wallet.address, r.signature);
  return r;
}
