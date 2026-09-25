// The Rust-produced vectors (permutation-server/tests/codec_vectors.rs) and
// helpers to compare decoded values with their JSON form.
import { readFileSync } from 'node:fs';

export const vectors = JSON.parse(readFileSync(new URL('./vectors.json', import.meta.url), 'utf8'));
export const fromHex = h => Uint8Array.from(Buffer.from(h, 'hex'));
export const hex = b => Buffer.from(b).toString('hex');

/** A decoded value as the vectors write it: bigints as decimal strings, bytes as hex. */
export const plain = v => JSON.parse(JSON.stringify(v, (_, x) => (typeof x === 'bigint' ? x.toString() : x instanceof Uint8Array ? hex(x) : x)));
