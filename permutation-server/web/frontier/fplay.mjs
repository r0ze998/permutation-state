// Play actions through the relay (contract §8.3, §9.4; web design §5): one
// path for every player and settle shape the page sends.
//
//   GET /f/relay (fee payer, blockhash) → the Frontier instruction built
//   from addresses recomputed here (accountsFor) → the legacy message with
//   the budgets' compute-budget prefix at CU price 0 → the pre-signing
//   check (fchainio.messageProblems: exact accounts and data, the relay's
//   own allowlist) → one signature (the session key; the wallet only for
//   Join and SetSession; nobody for a settle shape) → POST → the answer's
//   signature must be the announced fee payer's signature **of this
//   message** (a relay that co-signed anything else is caught before the
//   page reports success) → GET /f/tx until landed, failed or expired.
//
// A Reveal is never built here (I-24): marchbook.revealMaterial goes to
// POST /gw/f/reveal. Every function answers `{ok, …}` and never rejects.
import { frontierIx } from '../sdk/frontier/shapes.mjs';
import { budgetOf, budgetPrefix } from '../sdk/frontier/budgets.mjs';
import { compileMessage, wireTransaction } from '../sdk/solana-tx.mjs';
import { decode as fromBase58 } from '../sdk/base58.mjs';
import { toBase64 } from '../sdk/bytes.mjs';
import { verify } from '../session.mjs';
import * as io from './fchainio.mjs';
import { joinShardOf } from './faddr.mjs';

/**
 * ClashInputs' `camp_mask` (v1.7, I-56): u32 LE at offset 76 of the raw
 * account bytes (the herald's /h/clash `inputs_b64`). Since ABI v1.8 the
 * decoded ClashInputs carries it as `campMask`; this reader is the fallback
 * for raw bytes.
 */
export const CAMP_MASK_OFFSET = 76;
export function campMaskOf(bytes) {
  const b = bytes instanceof Uint8Array ? bytes : null;
  if (!b || b.length < CAMP_MASK_OFFSET + 4) return 0;
  return (b[76] | (b[77] << 8) | (b[78] << 16) | (b[79] << 24)) >>> 0;
}
/**
 * fclient::ix::camp_winner: the arrival at (faction, i) earns the camp's
 * Works when its fate is Stays (1) and its position `faction·4 + i` is the
 * lowest set bit of `camp_mask` (one winner per camp).
 */
export const campWinner = (mask, faction, i, fate) => fate === 1 && mask !== 0 && 31 - Math.clz32(mask & -mask) === faction * 4 + i;

/** The shapes this page sends and who signs them. */
export const SIGNED_BY = Object.freeze({
  Join: 'wallet', SetSession: 'wallet',
  SetVigil: 'session', FileTicket: 'session', Harvest: 'session', Build: 'session', Train: 'session', Muster: 'session',
  Dissolve: 'session', Garrison: 'session', Explore: 'session', Depart: 'session',
  SettleExplore: 'none', SettleTransit: 'none',
});
/** Build item of the walls (catalog `ITEM_WALLS`; the only Build that names the Province). */
export const ITEM_WALLS = 6;

const fail = (code, error, extra = {}) => ({ ok: false, code, error: error ?? code, ...extra });

/** The distinct provinces of a site list, in first-named order (FileTicket's province accounts). */
export function ticketProvinces(sites) {
  const out = [];
  for (const s of sites) if (!out.some(o => o.p === s.p && o.q === s.q)) out.push({ p: s.p, q: s.q });
  return out;
}

/**
 * The accounts of a shape (every one but `payer`, which is the relay's fee
 * payer), recomputed from the pinned season's addresses (faddr.mjs):
 *   v = {addresses, wallet, actor, faction?, joinGate?, holding: {p, q, site}?,
 *        province: {p, q}?, sites?, item?, settle?: {…keys the herald gave}}
 * `actor` is the signing key (the session key, or the wallet). Throws on a
 * shape this page does not send.
 */
export function accountsFor(name, v) {
  if (!SIGNED_BY[name]) throw new Error(`fplay: ${name} is not a shape this page sends`);
  const A = v.addresses;
  const citizen = A.of('Citizen', { wallet: v.wallet });
  const base = { actor: v.actor, season: A.season, citizen };
  const holding = () => A.of('Holding', v.holding);
  const province = () => A.of('Province', v.province ?? v.holding);
  switch (name) {
    case 'Join':
      return { wallet: v.wallet, season: A.season, frontier: A.of('Frontier'), citizen,
        joinshard: A.of('JoinShard', { faction: v.faction, shard: joinShardOf(v.wallet) }), ...(v.joinGate ? { join_gate: v.joinGate } : {}) };
    case 'SetSession':
      return { ...base, actor: v.wallet };
    case 'SetVigil':
      return base;
    case 'FileTicket':
      return { ...base, frontier: A.of('Frontier'), province: ticketProvinces(v.sites).map(p => A.of('Province', p)) };
    case 'Harvest': case 'Train':
      return { ...base, holding: holding() };
    case 'Build':
      return { ...base, holding: holding(), ...(v.item === ITEM_WALLS ? { province: province() } : {}) };
    case 'Muster': case 'Dissolve': case 'Garrison': case 'Explore': case 'Depart':
      return { ...base, holding: holding(), province: province() };
    case 'SettleExplore':
      return { season: A.season, holding: holding(), citizen, seedcache_or_archive: v.settle.seed, anchor_or_archive: v.settle.anchor };
    case 'SettleTransit': {
      const s = v.settle;
      // v1.7 (I-56): the camp's winner names its owner's Citizen as the optional 14th account.
      return { season: A.season, holding: holding(), dest_province: A.of('Province', s.dest), inputs: A.of('ClashInputs', { ...s.dest, bell: s.arriveBell }),
        slot: A.of('ArrivalSlot', { ...s.dest, bell: s.arriveBell, faction: s.faction, i: s.slotIndex ?? 0 }), home_province: A.of('Province', v.holding),
        anchor_or_archive: s.anchor, slot_beneficiary: s.slotBeneficiary, resolver: s.resolver, holding_rent_payer: s.rentPayer, settle_beneficiary: s.beneficiary,
        ...(s.campCitizen ? { citizen: s.campCitizen } : {}) };
    }
    default:
      throw new Error(`fplay: ${name} is not a shape this page sends`);
  }
}

/**
 * The message a shape would be (no signature): `{message, expected, budget,
 * signers}` for `feePayer` and `blockhash`. `signers` are the instruction's
 * own signing keys besides the fee payer.
 */
export function buildShape({ programId, name, accounts, fields, feePayer, blockhash }) {
  const expected = frontierIx(programId, name, { ...accounts, payer: feePayer }, fields);
  const message = compileMessage({ feePayer, recentBlockhash: blockhash, instructions: [...budgetPrefix(name), expected] });
  const signers = [];
  for (const k of expected.keys) if (k.isSigner && k.pubkey !== feePayer && !signers.includes(k.pubkey)) signers.push(k.pubkey);
  return { message, expected, budget: budgetOf(name), signers };
}

/**
 * Send one shape through the relay:
 *   {name, accounts, fields, signer: {session}|{wallet}|null, requester?: session key,
 *    citizen?: Citizen address (a settle's requester), invite?}
 * (`accounts` and `fields` may be functions of the relay's fee payer.)
 * → `{ok: true, signature, message, feePayer, lastValidBlockHeight, bytes}` or `{ok: false, code, error, …}`.
 */
export async function submit(req) {
  const pin = io.pinned();
  if (!pin) return fail('NoPin');
  const info = await io.relayInfo();
  if (!info.ok) return info;
  const { feePayer, blockhash, lastValidBlockHeight } = info;
  let built;
  try {
    // A settle names the fee payer inside its accounts and data (its beneficiary): functions of it.
    const accounts = typeof req.accounts === 'function' ? req.accounts(feePayer) : req.accounts;
    const fields = typeof req.fields === 'function' ? req.fields(feePayer) : (req.fields ?? {});
    built = buildShape({ programId: pin.programId, name: req.name, accounts, fields, feePayer, blockhash });
  } catch (e) {
    return fail('BuildFailed', String(e.message ?? e));
  }
  const { message, expected, budget, signers } = built;
  // A gated Join also names the gate key, which the relay signs (§8.3).
  const problems = io.messageProblems(message, { feePayer, blockhash, tag: expected.data[0], signers, cuLimit: budget.cuLimit, loadedLimit: budget.loadedLimit, expected });
  if (problems.length) return fail('MessageRefused', problems.join('; '), { problems });
  let wire;
  const extra = {};
  try {
    if (req.signer?.session) {
      const key = req.signer.session;
      wire = wireTransaction(message, { [key.publicKey]: await key.sign(message) });
    } else if (req.signer?.wallet) {
      wire = await req.signer.wallet.signTransaction(wireTransaction(message));
      if (!io.sameMessage(message, wire)) return fail('WalletAlteredMessage');
    } else {
      wire = wireTransaction(message);
      if (req.requester) {
        extra.requester = req.requester.publicKey;
        extra.requesterSig = toBase64(await req.requester.sign(message));
        if (req.citizen) extra.citizen = req.citizen;
      }
    }
  } catch (e) {
    return fail(e?.code ?? 'SignFailed', String(e?.message ?? e));
  }
  if (wire.length > budget.txCeiling) return fail('TooLarge', `${wire.length} B over the ${budget.txCeiling}-B ceiling of ${req.name}`);
  const r = req.name === 'Join'
    ? await io.sendJoin(wire, req.invite ?? null, { extra: { lastValidBlockHeight } })
    : await io.sendTx(wire, { extra: { ...extra, lastValidBlockHeight } });
  if (!r.ok) return r;
  // The answer names the transaction by its first signature: the fee
  // payer's. It must sign exactly the message this page built and signed.
  let sig;
  try { sig = fromBase58(String(r.signature)); } catch { sig = null; }
  if (!sig || sig.length !== 64 || !(await verify(feePayer, message, sig))) return fail('RelayMessageChanged');
  return { ok: true, signature: r.signature, message, feePayer, lastValidBlockHeight, bytes: wire.length };
}

const sleep = ms => new Promise(r => setTimeout(r, ms));

/**
 * Follow a sent transaction: `{ok: true, state: 'landed', slot}` or
 * `{ok: false, state: 'failed'|'expired'|'unknown', code, programCode?}`.
 */
export async function track(signature, { tries = 40, every = 1500, wait = sleep } = {}) {
  for (let i = 0; i < tries; i++) {
    const r = await io.txStatus(signature);
    if (r.ok && r.state === 'landed') return { ok: true, state: 'landed', slot: r.slot };
    if (r.ok && r.state === 'failed') return { ok: false, state: 'failed', code: r.code ?? 'TransactionFailed', programCode: r.programCode };
    if (r.ok && r.state === 'expired') return { ok: false, state: 'expired', code: 'Expired' };
    if (i + 1 < tries) await wait(every);
  }
  return { ok: false, state: 'unknown', code: 'Unconfirmed' };
}
