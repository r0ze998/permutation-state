// The SPL Token v3 instructions needed for local test USDC, built by hand
// (no @solana/spl-token dependency), and associated token accounts (the
// address and the create-idempotent instruction are the SDK's, player.mjs:
// the gateway and the browser build the same ones).
import { PublicKey, SystemProgram, TransactionInstruction } from '@solana/web3.js';
import { u64le } from '../client/src/bytes.mjs';
import { TOKEN_PROGRAM_ID } from '../client/src/pda.mjs';
import { ata, createAtaIdempotentIx as ataIx } from '../client/src/player.mjs';

export { TOKEN_PROGRAM_ID };
export const MINT_LEN = 82;
export const ACCOUNT_LEN = 165;

/** A player.mjs instruction (base58 keys, byte data) as a web3.js TransactionInstruction. */
export const web3Ix = ix => new TransactionInstruction({
  programId: new PublicKey(ix.programId),
  keys: ix.keys.map(k => ({ pubkey: new PublicKey(k.pubkey), isSigner: k.isSigner, isWritable: k.isWritable })),
  data: Buffer.from(ix.data),
});

/** `owner`'s associated token account for `mint` (a PublicKey). */
export const associatedTokenAccount = (owner, mint) => new PublicKey(ata(new PublicKey(owner).toBase58(), new PublicKey(mint).toBase58()));

/** Create `owner`'s associated token account for `mint` unless it exists; `payer` pays the rent. */
export const createAtaIdempotentIx = ({ payer, owner, mint }) =>
  web3Ix(ataIx({ payer: new PublicKey(payer).toBase58(), owner: new PublicKey(owner).toBase58(), mint: new PublicKey(mint).toBase58() }));

export async function createMintIxs(connection, { payer, mint, authority, decimals = 6 }) {
  const lamports = await connection.getMinimumBalanceForRentExemption(MINT_LEN);
  const data = Buffer.alloc(1 + 1 + 32 + 1);
  data[0] = 20; data[1] = decimals; authority.toBuffer().copy(data, 2); data[34] = 0; // InitializeMint2, no freeze authority
  return [
    SystemProgram.createAccount({ fromPubkey: payer, newAccountPubkey: mint, lamports, space: MINT_LEN, programId: TOKEN_PROGRAM_ID }),
    new TransactionInstruction({ programId: TOKEN_PROGRAM_ID, keys: [{ pubkey: mint, isSigner: false, isWritable: true }], data }),
  ];
}

export async function createTokenAccountIxs(connection, { payer, account, mint, owner }) {
  const lamports = await connection.getMinimumBalanceForRentExemption(ACCOUNT_LEN);
  const data = Buffer.alloc(33); data[0] = 18; owner.toBuffer().copy(data, 1); // InitializeAccount3
  return [
    SystemProgram.createAccount({ fromPubkey: payer, newAccountPubkey: account, lamports, space: ACCOUNT_LEN, programId: TOKEN_PROGRAM_ID }),
    new TransactionInstruction({ programId: TOKEN_PROGRAM_ID, keys: [{ pubkey: account, isSigner: false, isWritable: true }, { pubkey: mint, isSigner: false, isWritable: false }], data }),
  ];
}

export function mintToIx({ mint, dest, authority, amount }) {
  const data = Buffer.alloc(9); data[0] = 7; data.set(u64le(amount), 1);
  return new TransactionInstruction({ programId: TOKEN_PROGRAM_ID, keys: [
    { pubkey: mint, isSigner: false, isWritable: true }, { pubkey: dest, isSigner: false, isWritable: true }, { pubkey: authority, isSigner: true, isWritable: false }], data });
}

/** Token account balance (SPL layout: amount at 64..72). */
export async function tokenBalance(connection, account) {
  const info = await connection.getAccountInfo(account, 'confirmed');
  return info ? info.data.readBigUInt64LE(64) : 0n;
}

/** The token accounts `owner` holds of `mint`: [{address (base58), amount (bigint)}], largest first. */
export async function tokenAccountsOf(connection, owner, mint) {
  const r = await connection.getTokenAccountsByOwner(new PublicKey(owner), { mint: new PublicKey(mint) }, 'confirmed');
  return (r?.value ?? [])
    .map(({ pubkey, account }) => ({ address: new PublicKey(pubkey).toBase58(), amount: Buffer.from(account.data).readBigUInt64LE(64) }))
    .sort((a, b) => (a.amount === b.amount ? 0 : a.amount > b.amount ? -1 : 1));
}
