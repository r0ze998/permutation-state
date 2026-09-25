// Byte helpers shared by the client and the gateway: hex, little-endian
// u64s and JSON for values that hold bigints and bytes. One copy, so every
// module writes the same bytes and the same JSON (test/vectors.json pins the
// codec's; the state file and HTTP bodies depend on the JSON shape).

/** Bytes (Uint8Array, Buffer or byte array) as lowercase hex. */
export const toHex = b => Buffer.from(b).toString('hex');

/** Hex as a plain Uint8Array (not a Buffer: callers compare and store it as bytes). */
export const fromHex = h => new Uint8Array(Buffer.from(h, 'hex'));

/** A u64 (number or bigint) as 8 little-endian bytes, as Borsh and the program's seeds write it. */
export function u64le(v) {
  const b = new Uint8Array(8);
  new DataView(b.buffer).setBigUint64(0, BigInt(v), true);
  return b;
}

/**
 * JSON.stringify replacer: u64s as decimal strings, bytes as hex (JSON has
 * neither). A Buffer is turned into {type, data} by its own toJSON before a
 * replacer sees it, so only plain Uint8Arrays become hex.
 */
export const jsonSafe = (_, v) => (typeof v === 'bigint' ? v.toString() : v instanceof Uint8Array ? toHex(v) : v);

/** `value` as JSON, with `jsonSafe`. */
export const toJson = value => JSON.stringify(value, jsonSafe);
