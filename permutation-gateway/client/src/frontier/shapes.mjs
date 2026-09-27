// The transaction shapes the relay sponsors (contract §8.3, §9.4) — one
// definition for the relay (which refuses anything else) and the web client
// (which builds exactly these):
//
//   player  [SetComputeUnitLimit(budget), SetComputeUnitPrice(0), SetLoadedAccountsDataSizeLimit(L(kind)),
//            one Frontier player instruction ∈ {0x30–0x33, 0x40–0x46, 0x50}]
//            signers: the relay's fee payer, then the instruction's authority (Join: the wallet, and
//            the join gate when the season has one; every other: the actor, a session key or the wallet)
//   settle  [the same prefix, one of {0x47 SettleExplore, 0x54 SettleTransit}]
//            signers: the relay's fee payer only
//
// In both, the instruction's `payer` account is the fee payer. A Reveal
// (0x51) never goes through the relay as a transaction: `UseRevealRoute`
// (I-24). Accounts are checked against the ABI's list (count, order of
// signers and writability, the system program), and the message carries no
// other key and no other writability (v1.3); the program recomputes every
// address itself, so the relay does not.
import { budgetOf, budgetPrefix, COMPUTE_BUDGET_PROGRAM, PACKET_BYTES, parseComputeBudgetIx } from './budgets.mjs';
import { accountSpec, decodeIxData, encodeIxData, ixOf, TAGS } from './codec.mjs';
import { SYSTEM_PROGRAM } from './addresses.mjs';
import { compileMessage } from '../solana-tx.mjs';

export const REVEAL_TAG = 0x51;
export const JOIN_TAG = 0x30;
export const DEPART_TAG = 0x50;
export const FILE_TICKET_TAG = 0x33;
/** Tags of the player shapes (from the ABI's `relay_player_shape`). */
export const PLAYER_TAGS = Object.freeze(TAGS.instructions.filter(i => i.relay_player_shape).map(i => i.tag));
/** Tags of the settle shapes (`relay_settle_shape`). */
export const SETTLE_TAGS = Object.freeze(TAGS.instructions.filter(i => i.relay_settle_shape).map(i => i.tag));

const refuse = (code, problem) => ({ ok: false, code, problem });

/**
 * Classify a parsed legacy transaction (solana-tx.mjs `parseTransaction`)
 * against the sponsored shapes of `programId`. With `wireBytes`, its size is
 * checked against the kind's ceiling. Returns
 * `{ok: true, kind: 'player'|'settle', tag, name, feePayer, authority, signers, accounts: {name → key}, keys, data, budget, cu}`
 * or `{ok: false, code: 'UseRevealRoute'|'RelayRejected', problem}`.
 */
export function classify(tx, { programId, wireBytes } = {}) {
  const ixs = tx.instructions;
  // A Reveal anywhere is refused first, whatever else the transaction holds.
  if (ixs.some(ix => ix.programId === programId && ix.data[0] === REVEAL_TAG)) {
    return refuse('UseRevealRoute', 'a Reveal is not relayed as a transaction: POST /f/reveal with the reveal material');
  }
  if (ixs.length !== 4) return refuse('RelayRejected', 'exactly four instructions: the compute-budget prefix (limit, price, loaded data) and one Frontier instruction');
  const cb = ixs.slice(0, 3).map(parseComputeBudgetIx);
  if (cb.some(x => !x) || cb[0].kind !== 'limit' || cb[1].kind !== 'price' || cb[2].kind !== 'loaded') {
    return refuse('RelayRejected', 'the prefix must be SetComputeUnitLimit, SetComputeUnitPrice, SetLoadedAccountsDataSizeLimit, in that order');
  }
  const ix = ixs[3];
  if (ix.programId !== programId) return refuse('RelayRejected', 'the fourth instruction must be the Frontier program\'s');
  const tag = ix.data[0];
  const kind = PLAYER_TAGS.includes(tag) ? 'player' : SETTLE_TAGS.includes(tag) ? 'settle' : null;
  if (!kind) return refuse('RelayRejected', `instruction tag 0x${(tag ?? 0).toString(16)} is not a sponsored shape`);
  const abi = ixOf(tag);
  const budget = budgetOf(tag);
  const [limit, price, loaded] = cb.map(x => x.value);
  if (price !== 0n) return refuse('RelayRejected', 'SetComputeUnitPrice must be 0 on a sponsored shape');
  if (limit !== budget.cuLimit) return refuse('RelayRejected', `SetComputeUnitLimit must be ${budget.cuLimit} for ${abi.name} (the budgets table)`);
  if (loaded !== budget.loadedLimit) return refuse('RelayRejected', `SetLoadedAccountsDataSizeLimit must be L(${abi.name}) = ${budget.loadedLimit}`);
  if (wireBytes !== undefined && wireBytes > Math.min(budget.txCeiling, PACKET_BYTES)) {
    return refuse('RelayRejected', `${wireBytes} bytes is over ${abi.name}'s ceiling of ${budget.txCeiling}`);
  }
  let data;
  try { data = decodeIxData(ix.data); } catch (e) { return refuse('RelayRejected', `malformed ${abi.name} data: ${e.message}`); }
  const spec = accountSpec(tag, ix.keys.length);
  if (!spec) return refuse('RelayRejected', `${abi.name} does not take ${ix.keys.length} accounts`);
  const feePayer = tx.signers[0];
  const accounts = {};
  const signersWanted = [];
  for (let i = 0; i < spec.length; i++) {
    const want = spec[i];
    const have = ix.keys[i];
    // (A key in a non-signing position may still be a signer of the
    // transaction, e.g. the relay as SettleTransit's beneficiary; the
    // signer set itself is checked below.)
    if (want.signer && !have.isSigner) return refuse('RelayRejected', `${abi.name} account ${i} (${want.name}) must sign`);
    if (want.writable === 'w' && !have.isWritable) return refuse('RelayRejected', `${abi.name} account ${i} (${want.name}) must be writable`);
    if (want.kind === 'system' && have.pubkey !== SYSTEM_PROGRAM) return refuse('RelayRejected', `${abi.name} account ${i} must be the system program`);
    if (want.signer && !signersWanted.includes(have.pubkey)) signersWanted.push(have.pubkey);
    if (accounts[want.name] === undefined) accounts[want.name] = have.pubkey;
    else accounts[want.name] = [accounts[want.name], have.pubkey].flat();
  }
  if (accounts.payer !== feePayer) return refuse('RelayRejected', 'the instruction\'s payer account must be the fee payer');
  // The message's account keys are exactly the fee payer, the instruction's
  // accounts and the two programs, and every key's writability is what the
  // ABI gives it (writable iff the fee payer or at a writable position):
  // an unreferenced key, or a read-only account marked writable, would be a
  // relay-paid write lock the program never sees (integ-W2 review of W2-D).
  const wantWritable = new Map([[feePayer, true]]);
  for (let i = 0; i < spec.length; i++) {
    const k = ix.keys[i].pubkey;
    wantWritable.set(k, (wantWritable.get(k) ?? false) || spec[i].writable === 'w');
  }
  for (const prog of [COMPUTE_BUDGET_PROGRAM, programId]) {
    if (wantWritable.has(prog)) return refuse('RelayRejected', `${abi.name} may not pass a program id (${prog}) as an account`);
    wantWritable.set(prog, false);
  }
  if (tx.accountKeys.length !== wantWritable.size || tx.accountKeys.some(k => !wantWritable.has(k))) {
    return refuse('RelayRejected', 'the message names account keys no instruction of the shape uses');
  }
  const { numRequiredSignatures: ns, numReadonlySignedAccounts: rs, numReadonlyUnsignedAccounts: ru } = tx.header;
  const n = tx.accountKeys.length;
  for (let i = 0; i < n; i++) {
    const writable = i < ns ? i < ns - rs : i - ns < n - ns - ru;
    if (writable !== wantWritable.get(tx.accountKeys[i])) {
      return refuse('RelayRejected', `account ${tx.accountKeys[i]} must be ${wantWritable.get(tx.accountKeys[i]) ? 'writable' : 'read-only'} (the ABI's flags)`);
    }
  }
  const authority = kind === 'player' ? ix.keys[0].pubkey : null;
  if (authority === feePayer) return refuse('RelayRejected', 'the fee payer cannot also be the authority');
  // Exactly the fee payer and the instruction's own signers sign.
  const wanted = [feePayer, ...signersWanted.filter(k => k !== feePayer)];
  if (tx.signers.length !== wanted.length || wanted.some(k => !tx.signers.includes(k))) {
    return refuse('RelayRejected', kind === 'settle' ? 'a settle shape is signed by the fee payer only' : 'the fee payer and the instruction\'s signers sign, nobody else');
  }
  return { ok: true, kind, tag, name: abi.name, feePayer, authority, signers: tx.signers, accounts, keys: ix.keys, data, budget, cu: { limit, price, loaded } };
}

// ------------------------------------------------------------------ building

/**
 * A Frontier instruction from its ABI name, `accounts` (account name →
 * base58 key; a repeated group, e.g. FileTicket's provinces, as an array;
 * an absent optional one omitted) and data fields (codec `encodeIxData`).
 * Flags come from the ABI list.
 */
export function frontierIx(programId, name, accounts, fields) {
  const abi = ixOf(name);
  if (!abi) throw new Error(`unknown instruction ${name}`);
  const keys = [];
  for (const g of abi.account_groups) {
    const given = a => accounts[a.name] ?? (a.kind === 'system' ? SYSTEM_PROGRAM : undefined);
    const reps = Math.max(...g.accounts.map(a => [given(a)].flat().filter(x => x !== undefined).length));
    if (reps < g.min || reps > g.max) throw new Error(`${name}: ${reps} × ${g.accounts.map(a => a.name).join('+')}, the ABI allows ${g.min}–${g.max}`);
    for (let r = 0; r < reps; r++) {
      for (const a of g.accounts) {
        const k = [given(a)].flat()[r];
        if (k === undefined) throw new Error(`${name}: missing account ${a.name}`);
        keys.push({ pubkey: k, isSigner: a.signer, isWritable: a.writable === 'w' });
      }
    }
  }
  return { programId, keys, data: encodeIxData(name, fields) };
}

/**
 * The instructions of a sponsored shape: the budget prefix at CU price 0,
 * then `frontierIx(…)`.
 */
export const shapeIxs = (programId, name, accounts, fields) => [...budgetPrefix(name), frontierIx(programId, name, accounts, fields)];

/** The legacy message of a sponsored shape paid by `feePayer` (the relay's, from GET /f/relay). */
export const shapeMessage = ({ programId, name, accounts, fields, feePayer, recentBlockhash }) => compileMessage({ feePayer, recentBlockhash,
  instructions: shapeIxs(programId, name, { ...accounts, payer: accounts.payer ?? feePayer }, fields) });
