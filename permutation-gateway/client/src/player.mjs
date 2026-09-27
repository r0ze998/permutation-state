// A member's own transactions without web3.js (the browser builds, signs and
// sends them itself, like any agent): program addresses, Register (the
// wallet and the session key sign), Claim (the wallet signs), CommitOrders, RevealOrders and SubmitGov (the session
// key signs), the token account a prize goes to, and the seal receipt the
// gateway checks before it keeps a sealed batch.
//
// Every builder takes `programId` and `seasonId` and returns instructions in
// solana-tx.mjs form (keys as base58), account for account and byte for byte
// what chain.mjs builds (test/player.test.mjs compiles both). Compile them
// with `compileMessage({feePayer, recentBlockhash, instructions})`.
import { concat, fromHex, randomBytes, u16le, u64le, utf8 } from './bytes.mjs';
import { IX, MAX_GOV_PER_SIGNER, roleIndex, roleMask, SEEDS } from './codec.mjs';
import { COMPUTE_BUDGET_PROGRAM, computeBudgetHeapFrame, findProgramAddress, pubkeyBytes, pubkeyString } from './solana-tx.mjs';

export { COMPUTE_BUDGET_PROGRAM };
export const TOKEN_PROGRAM = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';
export const SYSTEM_PROGRAM = '11111111111111111111111111111111';
export const ASSOCIATED_TOKEN_PROGRAM = 'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';
/** Heap for anything that decodes a nation account (chain.mjs `medium`). */
export const SUBMIT_HEAP_BYTES = 128 * 1024;

// Deriving an address hashes until it falls off the curve; the few a page
// needs are kept (bounded, so a long-running caller does not grow forever).
const known = new Map();
function derive(programId, seeds) {
  const key = `${programId}|${seeds.map(s => Array.from(s).join(',')).join('|')}`;
  let a = known.get(key);
  if (!a) {
    if (known.size >= 1024) known.clear();
    a = findProgramAddress(seeds, programId)[0];
    known.set(key, a);
  }
  return a;
}

/** Program addresses of the season's accounts (base58), as pda.mjs derives them. */
export const pda = Object.freeze({
  season: (programId, seasonId) => derive(pubkeyString(programId), [utf8(SEEDS.season), u64le(seasonId)]),
  vault: (programId, seasonId) => derive(pubkeyString(programId), [utf8(SEEDS.vault), u64le(seasonId)]),
  /** One member per wallet per season. */
  member: (programId, seasonId, wallet) => derive(pubkeyString(programId), [utf8(SEEDS.member), u64le(seasonId), pubkeyBytes(wallet)]),
  /** A nation's office batches and governance inbox. */
  nation: (programId, seasonId, civ) => derive(pubkeyString(programId), [utf8(SEEDS.nation), u64le(seasonId), u16le(civ)]),
});

/** The associated token account of `owner` for `mint` (base58). */
export const ata = (owner, mint) => derive(ASSOCIATED_TOKEN_PROGRAM, [pubkeyBytes(owner), pubkeyBytes(TOKEN_PROGRAM), pubkeyBytes(mint)]);

const W = (pubkey, isSigner = false) => ({ pubkey: pubkeyString(pubkey), isSigner, isWritable: true });
const R = (pubkey, isSigner = false) => ({ pubkey: pubkeyString(pubkey), isSigner, isWritable: false });
const ix = (programId, keys, data) => ({ programId: pubkeyString(programId), keys, data });
const bytes32 = (b, what) => {
  const x = typeof b === 'string' ? fromHex(b) : Uint8Array.from(b);
  if (x.length !== 32) throw new Error(`${what}: 32 bytes`);
  return x;
};

/**
 * Register (the wallet and the session key sign; the entry fee and `deposit`
 * move from `walletToken`, a token account the wallet owns). `stand` is a
 * role mask or office names; `votes` default to nobody; `kind` defaults to 2
 * (undeclared), which every member of a season with operator AI members
 * must use; `tag` defaults to 32 random bytes.
 */
export function registerIx({ programId, seasonId, wallet, feePayer, civ, walletToken, mint, name, kind = 2, session, attestation, stand = 0, votes,
  deposit = 0n, tag = randomBytes(32) }) {
  return ix(programId, [R(wallet, true), W(feePayer, true), W(pda.season(programId, seasonId)), W(pda.member(programId, seasonId, wallet)), W(walletToken),
    W(pda.vault(programId, seasonId)), R(mint), R(TOKEN_PROGRAM), R(SYSTEM_PROGRAM), R(session, true)],
  IX.register({ civ, name, kind, session: pubkeyBytes(session), attestation, stand: Array.isArray(stand) ? roleMask(stand) : stand, votes,
    deposit: BigInt(deposit), tag }));
}

/** Claim a finalized season's prize and treasury share into `dest`, a token account the wallet owns (the wallet signs). */
export function claimIx({ programId, seasonId, wallet, dest, mint }) {
  return ix(programId, [R(wallet, true), W(pda.season(programId, seasonId)), W(pda.member(programId, seasonId, wallet)), W(pda.vault(programId, seasonId)),
    W(dest), R(mint), R(TOKEN_PROGRAM)], IX.claim());
}

/** Create `owner`'s associated token account for `mint` unless it exists (`payer` pays the rent and signs). */
export function createAtaIdempotentIx({ payer, owner, mint }) {
  return ix(ASSOCIATED_TOKEN_PROGRAM, [W(payer, true), W(ata(owner, mint)), R(owner), R(mint), R(SYSTEM_PROGRAM), R(TOKEN_PROGRAM)], Uint8Array.of(1));
}

const heap = () => computeBudgetHeapFrame(SUBMIT_HEAP_BYTES);

/** Seal one office's orders for the open tick: `[heap frame, CommitOrders]`, signed by the office holder's session key. */
export function commitOrdersIxs({ programId, seasonId, signer, civ, role, tick, commitment }) {
  return [heap(), ix(programId, [R(signer, true), W(pda.nation(programId, seasonId, civ))], IX.commitOrders({ role, tick, commitment: bytes32(commitment, 'commitment') }))];
}

/** Reveal a sealed batch in the reveal window: `[heap frame, RevealOrders]`. */
export function revealOrdersIxs({ programId, seasonId, signer, civ, role, tick, decisionDigest, orders, adopt = [], salt }) {
  return [heap(), ix(programId, [R(signer, true), W(pda.nation(programId, seasonId, civ))],
    IX.revealOrders({ role, tick, decisionDigest: bytes32(decisionDigest, 'decisionDigest'), orders, adopt, salt: bytes32(salt, 'salt') }))];
}

/** 1..MAX_GOV_PER_SIGNER governance actions of one member: `[heap frame, SubmitGov × n]`, signed by its session key. */
export function submitGovIxs({ programId, seasonId, signer, civ, member, actions }) {
  if (!actions?.length || actions.length > MAX_GOV_PER_SIGNER) throw new Error(`1–${MAX_GOV_PER_SIGNER} governance actions per transaction`);
  const nation = pda.nation(programId, seasonId, civ);
  return [heap(), ...actions.map(action => ix(programId, [R(signer, true), W(nation)], IX.submitGov({ member, action })))];
}

/**
 * The bytes an officer signs with its session key when it hands a sealed
 * batch to the gateway to reveal (POST /seal):
 * `"PS/seal/v1" ‖ season u64 ‖ tick u16 ‖ civ u16 ‖ role u8 ‖ commitment32`.
 * `role` is an office name or index; `commitment` bytes or hex.
 */
export function sealMessage({ seasonId, tick, civ, role, commitment }) {
  const r = typeof role === 'number' ? role : roleIndex(role);
  if (!Number.isInteger(r) || r < 0 || r > 3) throw new Error(`unknown office ${role}`);
  return concat(utf8('PS/seal/v1'), u64le(seasonId), u16le(tick), u16le(civ), [r], bytes32(commitment, 'commitment'));
}
