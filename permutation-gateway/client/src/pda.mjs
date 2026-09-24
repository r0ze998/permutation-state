import { PublicKey } from '@solana/web3.js';

export const TOKEN_PROGRAM_ID = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

const u64 = v => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(v)); return b; };
const u16 = v => { const b = Buffer.alloc(2); b.writeUInt16LE(v); return b; };

export const seasonPda = (programId, id) => PublicKey.findProgramAddressSync([Buffer.from('season'), u64(id)], programId)[0];
export const WORLD_CHUNKS = 20; // 4 KiB each (see permutation-chain state.rs)
export const worldChunkPda = (programId, id, k) => PublicKey.findProgramAddressSync([Buffer.from('world'), u64(id), Buffer.from([k])], programId)[0];
export const vaultPda = (programId, id) => PublicKey.findProgramAddressSync([Buffer.from('vault'), u64(id)], programId)[0];
/** A nation's office batches and governance inbox (delegated to the ER during play). */
export const nationPda = (programId, id, civ) => PublicKey.findProgramAddressSync([Buffer.from('nation'), u64(id), u16(civ)], programId)[0];
/** One member per wallet per season (base layer). */
export const memberPda = (programId, id, wallet) => PublicKey.findProgramAddressSync([Buffer.from('member'), u64(id), new PublicKey(wallet).toBuffer()], programId)[0];
