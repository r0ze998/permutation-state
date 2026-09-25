// Program-derived addresses of the permutation-chain accounts (seeds in
// codec.mjs `SEEDS`, checked against the Rust constants).
import { PublicKey } from '@solana/web3.js';
import { u64le } from './bytes.mjs';
import { SEEDS, WORLD_CHUNKS } from './codec.mjs';

export const TOKEN_PROGRAM_ID = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
export { WORLD_CHUNKS };

// Seed parts stay Buffers, like the `Buffer.from(SEEDS.x)` they sit next to.
const u64 = v => Buffer.from(u64le(v));
const u16 = v => { const b = Buffer.alloc(2); b.writeUInt16LE(v); return b; };
const pda = (programId, seeds) => PublicKey.findProgramAddressSync(seeds, programId)[0];

export const seasonPda = (programId, id) => pda(programId, [Buffer.from(SEEDS.season), u64(id)]);
/** World chunk `k` (0..WORLD_CHUNKS) of CHUNK bytes; chunk 0 starts with the world header. */
export const worldChunkPda = (programId, id, k) => pda(programId, [Buffer.from(SEEDS.world), u64(id), Buffer.from([k])]);
export const vaultPda = (programId, id) => pda(programId, [Buffer.from(SEEDS.vault), u64(id)]);
/** A nation's office batches and governance inbox (delegated to the ER during play). */
export const nationPda = (programId, id, civ) => pda(programId, [Buffer.from(SEEDS.nation), u64(id), u16(civ)]);
/** The operator's AI roster, revealed after the season (V5 §18.2). */
export const rosterPda = (programId, id) => pda(programId, [Buffer.from(SEEDS.roster), u64(id)]);
/** One member per wallet per season (base layer). */
export const memberPda = (programId, id, wallet) => pda(programId, [Buffer.from(SEEDS.member), u64(id), new PublicKey(wallet).toBuffer()]);
