// The SPL Token v3 instructions needed for local test USDC, built by hand
// (no @solana/spl-token dependency).
import { PublicKey, SystemProgram, TransactionInstruction } from '@solana/web3.js';

import { TOKEN_PROGRAM_ID } from '../client/src/pda.mjs';

export { TOKEN_PROGRAM_ID };
export const MINT_LEN = 82;
export const ACCOUNT_LEN = 165;

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
  const data = Buffer.alloc(9); data[0] = 7; data.writeBigUInt64LE(BigInt(amount), 1);
  return new TransactionInstruction({ programId: TOKEN_PROGRAM_ID, keys: [
    { pubkey: mint, isSigner: false, isWritable: true }, { pubkey: dest, isSigner: false, isWritable: true }, { pubkey: authority, isSigner: true, isWritable: false }], data });
}

/** Token account balance (SPL layout: amount at 64..72). */
export async function tokenBalance(connection, account) {
  const info = await connection.getAccountInfo(account, 'confirmed');
  return info ? info.data.readBigUInt64LE(64) : 0n;
}
