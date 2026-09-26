// The browser's chain actions, through the gateway's public routes — the
// same ones any x402 agent uses: read the season, test USDC, register over
// x402 (the wallet signs Register), relay session-signed transactions,
// seal receipts, talk, and claims (the wallet signs Claim; a wallet's
// prizes in this and earlier seasons come from GET /claims). The gateway
// pays every fee. Nothing here sends the member token, reaches an operator
// route or calls an RPC directly; the gateway is `view.chain.gateway ||
// lobby.gateway` (normally the play server's own `/gw`).
//
// Pinning: the program, cluster, season and entry fee come from the play
// server (`setPin`); the season's program addresses are derived here. A
// payment request, a fee payer and a relay answer are checked against them
// before any wallet is asked to sign, and after it signs the message must
// come back byte for byte, with a signature that verifies.
//
// Every call answers `{...body, ok, httpStatus, code, error}` (built from the
// HTTP status and the JSON body; a body's own `status`, such as the season's,
// is kept); i18n.mjs `errorText` translates `code`.
import { equal, toBase64, utf8 } from './sdk/bytes.mjs';
import { compileMessage, parseMessage, parseTransaction, pubkeyString, wireTransaction } from './sdk/solana-tx.mjs';
import { ata, claimIx, createAtaIdempotentIx, pda, registerIx } from './sdk/player.mjs';
import { claimParts } from './sdk/codec.mjs';
import { verify } from './session.mjs';
import { L } from './lang.mjs';

// ------------------------------------------------------------------ gateway and pins
let gw = '';
/** The gateway's base URL (e.g. `/gw`); set from the lobby or the view. */
export function setGateway(url) { if (url) gw = String(url).replace(/\/+$/, ''); }
export const gateway = () => gw;

let pin = null;
/**
 * Pin the season this page plays (from the play server's lobby/view, not
 * the gateway): `{programId, cluster, seasonId, entryFee, accounts?}`. The
 * season and vault addresses are derived here; `accounts` given by the
 * server must match them. Returns the pin, or throws.
 */
export function setPin({ programId, cluster, seasonId, entryFee, accounts }) {
  const p = {
    programId: pubkeyString(programId),
    cluster: String(cluster || 'localnet'),
    seasonId: String(BigInt(String(seasonId))),
    entryFee: BigInt(String(entryFee ?? 0)),
  };
  p.season = pda.season(p.programId, p.seasonId);
  p.vault = pda.vault(p.programId, p.seasonId);
  if ((accounts?.season && accounts.season !== p.season) || (accounts?.vault && accounts.vault !== p.vault)) {
    throw Object.assign(new Error(L`シーズンの口座がプログラムから導いたものと一致しません`), { code: 'PinMismatch' });
  }
  pin = Object.freeze(p);
  return pin;
}
export const pinned = () => pin;
/** `{cluster, programId, seasonId}` of the pinned season (session.mjs scope). */
export const scope = () => pin && { cluster: pin.cluster, programId: pin.programId, seasonId: pin.seasonId };

/** A failed result. */
export function fail(code, error, extra = {}) {
  return { ok: false, httpStatus: 0, code, error: error ?? code, ...extra };
}

// ------------------------------------------------------------------ HTTP
/** One gateway call; never rejects. */
const GET_TIMEOUT_MS = 12_000;
const POST_TIMEOUT_MS = 45_000;

export async function request(method, path, { body, headers = {} } = {}) {
  if (!gw) return fail('NoGateway', L`ゲートウェイの場所がわかりません`);
  let r;
  try {
    r = await fetch(`${gw}${path}`, {
      method, cache: 'no-store',
      // A relay waits for confirmation; a read that hangs must not freeze a card.
      signal: AbortSignal.timeout(method === 'GET' ? GET_TIMEOUT_MS : POST_TIMEOUT_MS),
      headers: body === undefined ? headers : { 'Content-Type': 'application/json', ...headers },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    return fail('network', 'network');
  }
  let json = null;
  try { json = await r.json(); } catch { /* not JSON */ }
  const b = json && typeof json === 'object' && !Array.isArray(json) ? json : {};
  const ok = r.ok && b.ok !== false;
  const code = ok ? (b.code ?? null) : (b.code ?? ({ 402: 'PaymentRequired', 429: 'RateLimited', 503: 'Unavailable' })[r.status] ?? `HTTP${r.status}`);
  return { ...b, ok, httpStatus: r.status, code, error: ok ? null : String(b.error ?? `HTTP ${r.status}`) };
}
const get = path => request('GET', path);
const post = (path, body, headers) => request('POST', path, { body, headers });

let lastSeason = null;
/** GET /season (fresh). Also remembered for the fee payer checks. */
export async function season() {
  const r = await get('/season');
  if (r.ok) lastSeason = r;
  return r;
}
/** The last /season answer, or null. */
export const lastSeasonInfo = () => lastSeason;
export const tick = () => get('/tick');
export const history = () => get('/history');
/** `{mint, decimals, accounts:[{address, amount}]}`: the owner's token accounts of the season mint. */
export const usdc = owner => get(`/usdc?owner=${encodeURIComponent(owner)}`);
/** Test USDC (localnet/devnet): exactly the entry fee + deposit, once per wallet per season. */
export const faucet = owner => post('/faucet', { owner });
/** Hand a sealed batch to the gateway to reveal (body per the /seal contract). */
export const seal = body => post('/seal', body);
/** A signed public message: `{member, to, text, tick, signature}`. */
export const talk = body => post('/talk', body);
/** Claim relay status of a season (this one by default): `{feePayer, blockhash, lastValidBlockHeight, mint, status}`. */
export const claimStatus = seasonId => get(`/claim-relay${seasonId == null ? '' : `?season=${encodeURIComponent(String(seasonId))}`}`);
/**
 * A wallet's members in this season and every season it follows, read by
 * the gateway: `{claims: [{seasonId, member, civ, name, amount (micro-USDC,
 * decimal string), claimed, status}]}`.
 */
export const claims = wallet => get(`/claims?wallet=${encodeURIComponent(String(wallet))}`);

/** Milliseconds from a time the gateway sends in seconds or milliseconds. */
const ms = t => (t == null || t === '' ? null : Number(t) > 1e12 ? Number(t) : Number(t) * 1000);
const big = (v, dflt = 0n) => { try { return v == null || v === '' ? dflt : BigInt(String(v)); } catch { return dflt; } };

/**
 * The registration window of a /season answer, times in ms: `{openedAt,
 * closesAt (null: no fixed deadline), serverNow, offsetMs (server − this
 * clock), members, aiCount, entryFee, deposit}`.
 */
export function registrationOf(s, now = Date.now()) {
  const r = s?.registration ?? {};
  const serverNow = ms(r.serverNow);
  return {
    openedAt: ms(r.openedAt), closesAt: ms(r.closesAt), serverNow, offsetMs: serverNow == null ? 0 : serverNow - now,
    members: Number(r.members ?? s?.members?.length ?? 0), aiCount: Number(r.aiCount ?? s?.season?.aiCount ?? 0),
    entryFee: big(r.entryFee ?? s?.season?.entryFee), deposit: big(r.deposit),
  };
}

/** Whether the gateway lets this page offer the dev wallet (`--dev-wallet`). */
export const devWalletAllowed = s => s?.devWallet === true || s?.registration?.devWallet === true;

/** x402 network name of a cluster (as the gateway names it). */
export const x402Network = cluster => (cluster === 'devnet' ? 'solana-devnet' : cluster === 'mainnet' || cluster === 'mainnet-beta' ? 'solana' : 'solana-localnet');

// ------------------------------------------------------------------ checks before any signature
/**
 * What is wrong with a 402's payment requirements for the pinned season
 * (empty if nothing). `mint` and `crank` come from /season, `need` is
 * entry fee + deposit, `wallet` the payer.
 */
export function paymentProblems(req, { pin: p = pin, mint, crank, need, wallet }) {
  const x = req?.extra ?? {};
  const out = [];
  if (!req || req.scheme !== 'exact') out.push('scheme');
  if (req?.network !== x402Network(p.cluster)) out.push('network');
  if (x.programId !== p.programId) out.push('programId');
  if (String(x.seasonId) !== p.seasonId) out.push('seasonId');
  if (x.accounts?.season !== p.season || x.accounts?.vault !== p.vault) out.push('accounts');
  if (req?.payTo !== p.vault) out.push('payTo');
  if (!mint || req?.asset !== mint) out.push('asset');
  if (big(req?.maxAmountRequired, -1n) !== need) out.push('amount');
  if (!crank || x.feePayer !== crank) out.push('feePayer');
  if (x.feePayer && x.feePayer === wallet) out.push('feePayer is the wallet');
  if (typeof x.recentBlockhash !== 'string' || !x.recentBlockhash) out.push('recentBlockhash');
  return out;
}

/**
 * What is wrong with a message the wallet is about to sign (null if
 * nothing): the fee payer first, exactly two signers (fee payer, wallet),
 * the wallet read-only, and only `programs`.
 */
export function walletMessageProblem(message, { feePayer, wallet, programs }) {
  let m;
  try { m = parseMessage(message); } catch (e) { return `message: ${e.message}`; }
  if (m.accountKeys[0] !== feePayer) return 'fee payer';
  if (m.signers.length !== 2 || m.signers[1] !== wallet || m.header.numReadonlySignedAccounts !== 1) return 'signers';
  if (m.instructions.some(ix => !programs.includes(ix.programId))) return 'programs';
  return null;
}

/**
 * Have the wallet sign `message`, then check it: the wire it returns must
 * carry the same message bytes, with the wallet's valid signature. Returns
 * the wire with only that signature (the gateway adds the fee payer's).
 * Throws `{code}` WalletModified / WalletBadSignature (or the wallet's own).
 */
export async function signByWallet(wallet, message) {
  const signed = await wallet.signTransaction(wireTransaction(message));
  let tx;
  try { tx = parseTransaction(signed); } catch { throw Object.assign(new Error(L`ウォレットの応答を取引として読めませんでした`), { code: 'WalletModified' }); }
  if (!equal(tx.message, message)) throw Object.assign(new Error(L`ウォレットが取引を書き換えました。署名は送っていません。`), { code: 'WalletModified' });
  const i = tx.signers.indexOf(wallet.address);
  if (i < 0 || !(await verify(wallet.address, message, tx.signatures[i]))) {
    throw Object.assign(new Error(L`ウォレットの署名を確認できませんでした`), { code: 'WalletBadSignature' });
  }
  return wireTransaction(message, { [wallet.address]: tx.signatures[i] });
}

/** A token account of `owner` holding at least `need` (the ATA preferred), or null. */
export function payingAccount(u, { owner, mint, need }) {
  const accounts = (u?.accounts || []).filter(a => big(a.amount) >= need);
  const mine = ata(owner, mint);
  return (accounts.find(a => a.address === mine) ?? accounts.sort((a, b) => (big(b.amount) > big(a.amount) ? 1 : -1))[0])?.address ?? null;
}
/** The best balance among `owner`'s accounts (micro-USDC). */
export const bestBalance = u => (u?.accounts || []).reduce((m, a) => (big(a.amount) > m ? big(a.amount) : m), 0n);

// ------------------------------------------------------------------ x402 registration
// A 402 that a fresh payment request and a new signature can fix.
const RETRYABLE = /settlement failed|blockhash|expired/i;

/**
 * Register over x402 with the connected wallet. `session` is the member's
 * session key (session.mjs), `stand` 1–2 office names; kind is 2
 * (undeclared), votes NOBODY, deposit the season's public default. Refuses
 * before any popup when the wallet already has a member or the session key
 * is taken. `onStep(stage)`: 'balance' | 'quote' | 'sign' | 'send'.
 * Resolves `{ok, member, civ, name, signature}` or a failure; `retry: true`
 * when a fresh 402 and a new signature can fix it (an expired blockhash).
 */
export async function join({ wallet, session, civ, name, stand, onStep = () => {} }) {
  const p = pin;
  if (!p) return fail('NoPin', L`シーズンがまだわかりません`);
  const s = await season();
  if (!s.ok) return s;
  if (s.programId !== p.programId || String(s.season?.seasonId) !== p.seasonId) return fail('X402Mismatch', L`ゲートウェイのシーズンがゲームサーバーのものと一致しません`);
  const members = s.members || [];
  if (members.some(m => m.wallet === wallet.address)) return fail('AlreadyMember', L`このウォレットはすでにこのシーズンの国民です`);
  if (members.some(m => m.session === session.publicKey)) return fail('SessionInUse');
  const { deposit } = registrationOf(s);
  const need = p.entryFee + deposit;
  const mint = s.season?.usdcMint, crank = s.season?.crank;
  onStep('balance');
  const u = await usdc(wallet.address);
  if (!u.ok) return u;
  const from = payingAccount(u, { owner: wallet.address, mint, need });
  if (!from) return fail('InsufficientFunds');
  onStep('quote');
  const first = await post('/x402/join', { civ, name });
  if (first.httpStatus !== 402) return first.ok ? fail('X402Mismatch', L`ゲートウェイが支払い条件を返しませんでした`) : first;
  const req = first.accepts?.[0];
  const problems = paymentProblems(req, { pin: p, mint, crank, need, wallet: wallet.address });
  if (problems.length) return fail('X402Mismatch', L`支払いの条件がこのシーズンと合いません（${problems.join(L`、`)}）`);
  const ix = registerIx({ programId: p.programId, seasonId: p.seasonId, wallet: wallet.address, feePayer: crank, civ, walletToken: from, mint, name,
    kind: 2, session: session.publicKey, stand, deposit });
  const message = compileMessage({ feePayer: crank, recentBlockhash: req.extra.recentBlockhash, instructions: [ix] });
  const shape = walletMessageProblem(message, { feePayer: crank, wallet: wallet.address, programs: [p.programId] });
  if (shape) return fail('X402Mismatch', L`取引の形が想定と違います（${shape}）`);
  onStep('sign');
  let wire;
  try { wire = await signByWallet(wallet, message); } catch (e) { return fail(e.code || 'WalletError', e.message); }
  onStep('send');
  const payment = { x402Version: 1, scheme: 'exact', network: req.network, payload: { transaction: toBase64(wire) } };
  const second = await post('/x402/join', { civ, name }, { 'X-PAYMENT': toBase64(utf8(JSON.stringify(payment))) });
  if (second.ok) return { ...second, ok: true };
  // The payment may have settled though the answer was lost: the member is the proof.
  const after = await season();
  const mine = after.ok && (after.members || []).find(m => m.wallet === wallet.address);
  if (mine) return { ok: true, httpStatus: 200, member: mine.index, civ: mine.civ, name: mine.name, recovered: true };
  if (second.code === 'BlockhashExpired' || second.code === 'SettlementFailed' || (second.httpStatus === 402 && RETRYABLE.test(second.error))) return { ...second, code: 'BlockhashExpired', retry: true };
  return second;
}

// ------------------------------------------------------------------ session-signed relays (ER)
async function crankKey() {
  if (!lastSeason) await season();
  return lastSeason?.season?.crank ?? null;
}

/**
 * Sign `ixs` (player.mjs commitOrdersIxs / submitGovIxs …) with the session
 * key and relay them on the ER; the gateway only adds the fee payer, which
 * must be the season's crank. A fresh blockhash per call.
 */
export async function relay(ixs, session) {
  const info = await get('/relay');
  if (!info.ok) return info;
  const crank = await crankKey();
  if (!crank || info.feePayer !== crank || info.feePayer === session.publicKey) return fail('RelayMismatch', L`ゲートウェイの手数料支払い者がシーズンのものと違います`);
  let message;
  try { message = compileMessage({ feePayer: info.feePayer, recentBlockhash: info.blockhash, instructions: ixs }); } catch (e) { return fail('InvalidTransaction', e.message); }
  const signature = await session.sign(message);
  const wire = wireTransaction(message, { [session.publicKey]: signature });
  return post('/relay', { tx: toBase64(wire), lastValidBlockHeight: info.lastValidBlockHeight });
}

// ------------------------------------------------------------------ claims (base layer)
/**
 * Where a prize goes: the wallet's ATA or another token account it already
 * has (from /usdc, when `mint` is the season mint); else the ATA, created
 * in the same transaction (`create: true`, paid by the gateway).
 */
export function claimDestination(u, { owner, mint }) {
  const mine = ata(owner, mint);
  const accounts = u?.mint === mint ? (u.accounts || []) : [];
  if (accounts.some(a => a.address === mine)) return { dest: mine, create: false };
  if (accounts.length) return { dest: accounts[0].address, create: false };
  return { dest: mine, create: true };
}

/**
 * Claim a finalized season's prize (and treasury share) to the connected
 * wallet: this season by default, or an earlier one of the gateway's
 * lineage. The wallet signs; the gateway pays the fee.
 */
export async function claim({ wallet, seasonId = pin?.seasonId, onStep = () => {} }) {
  const p = pin;
  if (!p) return fail('NoPin');
  onStep('status');
  const info = await claimStatus(seasonId);
  if (!info.ok) return info;
  if (info.status !== 'Finalized') return fail('NotFinalized', null, { seasonStatus: info.status });
  if (info.programId && info.programId !== p.programId) return fail('RelayMismatch', L`ゲートウェイのプログラムがシーズンのものと違います`);
  const crank = await crankKey();
  if (!crank || info.feePayer !== crank || info.feePayer === wallet.address) return fail('RelayMismatch', L`ゲートウェイの手数料支払い者がシーズンのものと違います`);
  const u = await usdc(wallet.address);
  const { dest, create } = claimDestination(u.ok ? u : null, { owner: wallet.address, mint: info.mint });
  const ixs = [
    ...(create ? [createAtaIdempotentIx({ payer: info.feePayer, owner: wallet.address, mint: info.mint })] : []),
    claimIx({ programId: p.programId, seasonId, wallet: wallet.address, dest, mint: info.mint }),
  ];
  const message = compileMessage({ feePayer: info.feePayer, recentBlockhash: info.blockhash, instructions: ixs });
  const shape = walletMessageProblem(message, { feePayer: info.feePayer, wallet: wallet.address, programs: [p.programId, ixs[0].programId] });
  if (shape) return fail('RelayMismatch', L`取引の形が想定と違います（${shape}）`);
  onStep('sign');
  let wire;
  try { wire = await signByWallet(wallet, message); } catch (e) { return fail(e.code || 'WalletError', e.message); }
  onStep('send');
  const r = await post('/claim-relay', { tx: toBase64(wire), lastValidBlockHeight: info.lastValidBlockHeight });
  return r.ok ? { ...r, dest } : r;
}

/** A member's claim from /season: `{prize, refund, total}` (bigints) for `member` (a /season members[] entry). */
export function claimOf(s, member) {
  if (!s?.season || !member) return null;
  const season = { payouts: s.season.payouts || [], treasury: s.season.treasury || [], treasuryFinal: s.season.treasuryFinal || [] };
  return claimParts(season, { index: member.index, civ: member.civ, shares: member.shares ?? 0 });
}

// ------------------------------------------------------------------ explorer
/** An explorer link for a transaction on the base layer (null when there is no public explorer for it). */
export function explorerTx(signature, { cluster = pin?.cluster, base } = {}) {
  const sig = encodeURIComponent(String(signature ?? ''));
  if (!sig) return null;
  if (cluster === 'devnet') return `https://explorer.solana.com/tx/${sig}?cluster=devnet`;
  if (cluster === 'mainnet' || cluster === 'mainnet-beta') return `https://explorer.solana.com/tx/${sig}`;
  return /^https?:\/\//.test(String(base ?? '')) ? `https://explorer.solana.com/tx/${sig}?cluster=custom&customUrl=${encodeURIComponent(base)}` : null;
}
