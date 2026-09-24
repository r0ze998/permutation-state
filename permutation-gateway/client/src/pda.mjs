import { PublicKey } from '@solana/web3.js';

export const TOKEN_PROGRAM_ID = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

const u64 = v => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(v)); return b; };
const u16 = v => { const b = Buffer.alloc(2); b.writeUInt16LE(v); return b; };

export const seasonPda = (programId, id) => PublicKey.findProgramAddressSync([Buffer.from('season'), u64(id)], programId)[0];
export const WORLD_CHUNKS = 8;
export const worldChunkPda = (programId, id, k) => PublicKey.findProgramAddressSync([Buffer.from('world'), u64(id), Buffer.from([k])], programId)[0];
export const vaultPda = (programId, id) => PublicKey.findProgramAddressSync([Buffer.from('vault'), u64(id)], programId)[0];
export const ordersPda = (programId, id, civ) => PublicKey.findProgramAddressSync([Buffer.from('orders'), u64(id), u16(civ)], programId)[0];
