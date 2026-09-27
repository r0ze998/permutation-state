// The write path: the relay behind the herald's `/gw/*` proxy (contract
// §8.3, §9.4; web design §5), in the pattern of v9's chainio.mjs. The
// relay pays every fee from a pool of ≥ 150 keys; the wallet signs only
// Join (and SetSession), the session key every other player action, and
// Reveal is not a transaction at all (I-24: the browser posts the reveal
// material, the keeper signs).
//
// Pinning: the program, cluster and season come from the herald's season
// record and are re-derived here (the Season PDA and its bump are found
// locally, faddr.mjs); the ruleset hash must be the pinned one. Before any
// key signs, a message is checked byte for byte: the announced fee payer
// first, the three compute-budget instructions with a zero CU price, one
// Frontier instruction of an allowed tag, the recent blockhash; after the
// relay co-signs, the message must come back unchanged.
//
// Every call answers `{…body, ok, httpStatus, code, error}` and never rejects.
import { equal, toBase64 } from '../sdk/bytes.mjs';
import { COMPUTE_BUDGET_PROGRAM, messageOf, parseMessage, pubkeyString } from '../sdk/solana-tx.mjs';
import { RULESET_HASH } from './abi.mjs';
import { classify } from '../sdk/frontier/shapes.mjs';
import { seasonAddresses } from './faddr.mjs';
import { L } from '../lang.mjs';

// ------------------------------------------------------------------ base URL and pins
let relayBase = '';
/** The relay's base URL as the page sees it (normally `/gw`). */
export function setRelay(url) { relayBase = String(url ?? '').replace(/\/+$/, ''); }
export const relay = () => relayBase;

let pin = null;
/**
 * Pin the season this page plays from the herald's season record and the
 * decoded Season account: `{programId, cluster, seasonId}` + addresses.
 * Throws `{code: 'PinMismatch' | 'RulesetMismatch'}`.
 */
export function setPin({ programId, cluster, seasonId, seasonAddress = null, rulesetHash = null }) {
  const addresses = seasonAddresses(programId, seasonId);
  if (seasonAddress && seasonAddress !== addresses.season) throw Object.assign(new Error(L`シーズンの口座がプログラムから導いたものと一致しません`), { code: 'PinMismatch' });
  if (rulesetHash !== null && String(rulesetHash).toLowerCase() !== RULESET_HASH) throw Object.assign(new Error(L`このシーズンは別のルールで動いています`), { code: 'RulesetMismatch' });
  pin = Object.freeze({ programId: addresses.programId, cluster: String(cluster || 'localnet'), seasonId: addresses.seasonId, addresses });
  return pin;
}
export const pinned = () => pin;
/** `{cluster, programId, seasonId}` (fsession / marchbook scope). */
export const scope = () => pin && { cluster: pin.cluster, programId: pin.programId, seasonId: pin.seasonId };

/** A failed result. */
export const fail = (code, error, extra = {}) => ({ ok: false, httpStatus: 0, code, error: error ?? code, ...extra });

// ------------------------------------------------------------------ HTTP
const TIMEOUT = { GET: 12_000, POST: 45_000 };
export async function request(method, path, { body, fetch: f = (...a) => globalThis.fetch(...a) } = {}) {
  if (!relayBase) return fail('NoRelay', L`中継の場所がわかりません`);
  let r;
  try {
    r = await f(`${relayBase}${path}`, {
      method, cache: 'no-store', signal: AbortSignal.timeout?.(TIMEOUT[method] ?? 12_000),
      headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    return fail('network', 'network');
  }
  let json = null;
  try { json = await r.json(); } catch { /* not JSON */ }
  const b = json && typeof json === 'object' && !Array.isArray(json) ? json : {};
  const ok = r.ok && b.ok !== false;
  const code = ok ? (b.code ?? null) : (b.code ?? ({ 429: 'QuotaExceeded', 503: 'Unavailable' })[r.status] ?? `HTTP${r.status}`);
  return { ...b, ok, httpStatus: r.status, code, error: ok ? null : String(b.error ?? `HTTP ${r.status}`) };
}

/** GET /f/relay: a fee payer from the pool, a blockhash, the quota. The program must be the pinned one. */
export async function relayInfo(opts) {
  const r = await request('GET', '/f/relay', opts);
  if (!r.ok) return r;
  if (!pin) return fail('NoPin', L`シーズンがまだ決まっていません`);
  try {
    if (pubkeyString(r.programId) !== pin.programId) return fail('PinMismatch', L`中継が別のプログラムを名乗っています`);
    pubkeyString(r.feePayer);
  } catch {
    return fail('BadRelayAnswer', L`中継の答えを読めませんでした`);
  }
  return r;
}

/** POST /f/relay with a wire transaction signed by every signer but the fee payer. */
export const sendTx = (wire, opts) => request('POST', '/f/relay', { ...opts, body: { tx: toBase64(wire) } });
/** POST /f/join: the wallet-signed Join (with an invite when the season is gated). */
export const sendJoin = (wire, invite, opts) => request('POST', '/f/join', { ...opts, body: invite ? { tx: toBase64(wire), invite } : { tx: toBase64(wire) } });
/** POST /f/reveal: reveal material, no signature (I-24; marchbook.revealMaterial). */
export const reveal = (material, opts) => request('POST', '/f/reveal', { ...opts, body: material });
/** POST /f/nudge: ask the keeper to catch a province up (a resident action waits on it). */
export const nudge = (p, q, bell, opts) => request('POST', '/f/nudge', { ...opts, body: { province: [p, q], bell } });
/** GET /f/tx/{signature}. */
export const txStatus = (sig, opts) => request('GET', `/f/tx/${encodeURIComponent(sig)}`, opts);
/** GET /f/quota?citizen=. */
export const quota = (citizen, opts) => request('GET', `/f/quota?citizen=${encodeURIComponent(citizen)}`, opts);

// ------------------------------------------------------------------ message checks (§9.4)
/** Player shapes (session or wallet signer) and settle shapes (no authority signer) the relay accepts (§8.3). */
export const PLAYER_TAGS = Object.freeze([0x30, 0x31, 0x32, 0x33, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x50]);
export const SETTLE_TAGS = Object.freeze([0x47, 0x54]);
export const REVEAL_TAG = 0x51;
const CB = { limit: 2, price: 3, loaded: 4, heap: 1 };
const u32 = (d, o) => d[o] | (d[o + 1] << 8) | (d[o + 2] << 16) | (d[o + 3] * 2 ** 24);
const u64zero = (d, o) => d.subarray(o, o + 8).every(x => x === 0);

/**
 * Problems with a message before a key signs it (empty = fine):
 * `{feePayer, blockhash, tag, signers: [base58], cuLimit?, loadedLimit?,
 * expected}`. `blockhash` (the one GET /f/relay gave) and `expected` (the
 * Frontier instruction this page built from addresses it recomputed,
 * faddr.mjs / the SDK's `frontierIx`) are required: the message's Frontier
 * instruction must name exactly `expected`'s accounts in order and carry
 * exactly its data (no bump from anyone), its `payer` account must be the
 * fee payer, and the message must be a relay shape as the relay checks it
 * (the SDK's `classify`: no unreferenced key, ABI writability) (§9.4;
 * integ-W2 review of W2-E). A Reveal is never a client transaction
 * (`UseRevealRoute`).
 */
export function messageProblems(message, { feePayer, blockhash, tag, signers = [], cuLimit = null, loadedLimit = null, expected = null }) {
  const out = [];
  let m;
  try { m = parseMessage(message); } catch (e) { return [`unparseable: ${e.message}`]; }
  if (!pin) return ['no pinned season'];
  if (m.accountKeys[0] !== feePayer) out.push('fee payer is not the announced relay key');
  if (!blockhash) out.push('no recent blockhash to check against (GET /f/relay gives it)');
  else if (m.recentBlockhash !== blockhash) out.push('recent blockhash differs');
  const want = [feePayer, ...signers];
  if (m.signers.length !== want.length || want.some(s => !m.signers.includes(s))) out.push(`signers ${m.signers.join(',')} are not ${want.join(',')}`);
  const ix = m.instructions;
  if (ix.length !== 4) out.push(`${ix.length} instructions, expected 3 compute-budget + 1`);
  const cb = ix.slice(0, 3);
  const kinds = cb.map(i => (i.programId === COMPUTE_BUDGET_PROGRAM ? i.data[0] : -1));
  if (JSON.stringify(kinds) !== JSON.stringify([CB.limit, CB.price, CB.loaded])) out.push('compute-budget prefix is not [limit, price, loaded-data limit]');
  if (cb[1] && (cb[1].data.length !== 9 || !u64zero(cb[1].data, 1))) out.push('CU price is not 0');
  if (cuLimit !== null && cb[0] && u32(cb[0].data, 1) !== cuLimit) out.push('CU limit differs from the budgets table');
  if (loadedLimit !== null && cb[2] && u32(cb[2].data, 1) !== loadedLimit) out.push('loaded-data limit differs from L(kind)');
  const f = ix[3];
  if (!f || f.programId !== pin.programId) out.push('the last instruction is not the Frontier program');
  else {
    const t = f.data[0];
    if (t === REVEAL_TAG) out.push('UseRevealRoute');
    else if (!PLAYER_TAGS.includes(t) && !SETTLE_TAGS.includes(t)) out.push(`tag 0x${t?.toString(16)} is not a player or settle shape`);
    else if (tag !== undefined && t !== tag) out.push(`tag 0x${t.toString(16)} is not the expected 0x${tag.toString(16)}`);
  }
  for (const i of ix) if (i.programId !== COMPUTE_BUDGET_PROGRAM && i.programId !== pin.programId) out.push(`unexpected program ${i.programId}`);
  if (f && f.programId === pin.programId && f.data[0] !== REVEAL_TAG) {
    const c = classify(m, { programId: pin.programId });
    if (!c.ok) out.push(`not a relay shape: ${c.problem}`);
    else if (c.accounts.payer !== feePayer) out.push('the instruction\'s payer account is not the fee payer');
    if (!expected) out.push('no expected instruction to compare (build it from recomputed addresses)');
    else {
      if (expected.programId !== f.programId) out.push('the Frontier program differs from the expected instruction\'s');
      const want = expected.keys.map(k => k.pubkey);
      const have = f.keys.map(k => k.pubkey);
      if (want.length !== have.length) out.push(`${have.length} accounts, the expected instruction has ${want.length}`);
      else for (let i = 0; i < want.length; i++) if (want[i] !== have[i]) out.push(`account ${i} is ${have[i]}, expected ${want[i]} (recomputed)`);
      if (!equal(Uint8Array.from(expected.data), f.data)) out.push('instruction data differs from the expected instruction\'s');
    }
  }
  return out;
}

/** After the relay answers with a co-signed wire: the message must be byte for byte the one this page signed. */
export function sameMessage(signedMessage, wire) {
  try { return equal(messageOf(wire), signedMessage); } catch { return false; }
}
