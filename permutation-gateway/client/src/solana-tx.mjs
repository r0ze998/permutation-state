// Legacy Solana transactions without web3.js, for the browser and for
// anything that must see the exact bytes a key signs: public keys, program
// addresses, compiling a message, the wire format and parsing it back.
//
// `compileMessage` is byte-identical to web3.js 1.99
// `Transaction.compileMessage().serialize()` (same account order, including
// its locale-aware sort of base58 keys), so a message built here and one
// built with web3.js are the same message (test/solana-tx.test.mjs).
//
// Keys are passed as base58 strings, 32 bytes, or anything with `toBytes()`
// (a web3.js PublicKey); every key this module returns is a base58 string.
// An instruction is `{programId, keys: [{pubkey, isSigner, isWritable}], data}`.
import { concat, u32le, utf8 } from './bytes.mjs';
import { decode as fromBase58, encode as toBase58 } from './base58.mjs';
import { sha256 } from './sha256.mjs';

export const COMPUTE_BUDGET_PROGRAM = 'ComputeBudget111111111111111111111111111111';
/** A transaction's size limit (one packet), in bytes. */
export const PACKET_BYTES = 1232;
/** Bytes per PDA seed at most. */
export const MAX_SEED_BYTES = 32;

/** A public key as its 32 bytes (a fresh copy). Throws unless it is one. */
export function pubkeyBytes(k) {
  let b;
  if (typeof k === 'string') b = fromBase58(k);
  else if (k instanceof Uint8Array || Array.isArray(k)) b = Uint8Array.from(k);
  else if (k && typeof k.toBytes === 'function') b = Uint8Array.from(k.toBytes());
  else throw new TypeError('not a public key');
  if (b.length !== 32) throw new Error(`invalid public key: ${b.length} bytes, not 32`);
  return b;
}

/** A public key as base58. Throws unless it is one. */
export const pubkeyString = k => toBase58(pubkeyBytes(k));

// ------------------------------------------------------------------ ed25519

const P = 2n ** 255n - 19n;
const D = 0x52036cee2b6ffe738cc740797779e89800700a4d4141d8ab75eb4dca135978a3n;
const sq = (x, n) => { for (let i = 0; i < n; i++) x = (x * x) % P; return x; };
/** x^((p-5)/8) mod p, by the addition chain RFC 8032 implementations use. */
function powP58(x) {
  const x2 = (x * x) % P;
  const b2 = (x2 * x) % P;
  const b4 = (sq(b2, 2) * b2) % P;
  const b5 = (sq(b4, 1) * x) % P;
  const b10 = (sq(b5, 5) * b5) % P;
  const b20 = (sq(b10, 10) * b10) % P;
  const b40 = (sq(b20, 20) * b20) % P;
  const b80 = (sq(b40, 40) * b40) % P;
  const b160 = (sq(b80, 80) * b80) % P;
  const b240 = (sq(b160, 80) * b80) % P;
  const b250 = (sq(b240, 10) * b10) % P;
  return (sq(b250, 2) * x) % P;
}

/**
 * Whether 32 bytes decode to a point of ed25519 (RFC 8032 §5.1.3, strict:
 * y < p, and x = 0 must not carry a sign bit), exactly as web3.js decides
 * with @noble/curves. A program address must not.
 */
function onCurve(b) {
  let y = 0n;
  for (let i = 31; i >= 0; i--) y = (y << 8n) | BigInt(i === 31 ? b[i] & 0x7f : b[i]);
  if (y >= P) return false;
  const y2 = (y * y) % P;
  const u = (y2 + P - 1n) % P; // y² − 1
  const v = (D * y2 + 1n) % P; // d·y² + 1 (never 0)
  const v3 = (((v * v) % P) * v) % P;
  const v7 = (((v3 * v3) % P) * v) % P;
  const x = (((u * v3) % P) * powP58((u * v7) % P)) % P;
  const vx2 = (((v * x) % P) * x) % P;
  // x² = u/v has a root iff v·x² is u or −u (then x·√−1 is the root).
  if (vx2 !== u && vx2 !== (P - u) % P) return false;
  return !(u === 0n && b[31] & 0x80);
}

/** Whether a public key is an ed25519 point (a wallet's key is; a program address is not). */
export const isOnCurve = k => onCurve(pubkeyBytes(k));

// ------------------------------------------------------------------ program addresses

const PDA_MARKER = utf8('ProgramDerivedAddress');
function seedBytes(s) {
  const b = typeof s === 'string' ? utf8(s) : Uint8Array.from(s);
  if (b.length > MAX_SEED_BYTES) throw new TypeError('Max seed length exceeded');
  return b;
}

/** The program address of exactly `seeds` (bytes; a string is UTF-8), or a throw when it is on the curve. */
export function createProgramAddress(seeds, programId) {
  const hash = sha256(...seeds.map(seedBytes), pubkeyBytes(programId), PDA_MARKER);
  if (onCurve(hash)) throw new Error('Invalid seeds, address must fall off the curve');
  return toBase58(hash);
}

/**
 * `[address, bump]`: the first program address off the curve for `seeds`
 * and a bump from 255 down to 1, as `PublicKey.findProgramAddressSync`.
 */
export function findProgramAddress(seeds, programId) {
  const parts = seeds.map(seedBytes);
  const program = pubkeyBytes(programId);
  for (let bump = 255; bump > 0; bump--) {
    const hash = sha256(...parts, [bump], program, PDA_MARKER);
    if (!onCurve(hash)) return [toBase58(hash), bump];
  }
  throw new Error('Unable to find a viable program address nonce');
}

// ------------------------------------------------------------------ messages

/** `RequestHeapFrame(bytes)` of the compute-budget program. */
export const computeBudgetHeapFrame = bytes => ({ programId: COMPUTE_BUDGET_PROGRAM, keys: [], data: concat([1], u32le(bytes)) });

/** Solana's compact-u16 length prefix. */
function shortvec(n) {
  if (!Number.isInteger(n) || n < 0 || n > 0xffff) throw new RangeError(`length ${n} does not fit a compact-u16`);
  const out = [];
  for (;;) {
    let b = n & 0x7f;
    n >>= 7;
    if (n === 0) { out.push(b); return out; }
    b |= 0x80;
    out.push(b);
  }
}
/** `[value, next offset]` of the compact-u16 at `o`; canonical encodings only, as the runtime reads them. */
function readShortvec(b, o) {
  let n = 0;
  for (let i = 0; i < 3; i++) {
    if (o + i >= b.length) throw new Error('transaction: truncated length');
    const x = b[o + i];
    if (i > 0 && x === 0) throw new Error('transaction: non-canonical length');
    n |= (x & 0x7f) << (7 * i);
    if (!(x & 0x80)) {
      if (n > 0xffff) throw new Error('transaction: length out of range');
      return [n, o + i + 1];
    }
  }
  throw new Error('transaction: bad length prefix');
}

// web3.js sorts keys of the same kind with
// `a.localeCompare(b, 'en', {sensitivity: 'variant', caseFirst: 'lower', …})`.
// For base58 (digits and ASCII letters) that collation is: compare case-blind
// with digits before letters, a shorter prefix first; then, at the first
// position whose case differs, lower case first. Written out here so every
// runtime orders keys the same (checked against localeCompare in the tests).
const weight = c => (c <= 57 ? c : c >= 97 ? c - 32 : c); // digits < letters; case folded
export function compareKeys(a, b) {
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) {
    const d = weight(a.charCodeAt(i)) - weight(b.charCodeAt(i));
    if (d) return d < 0 ? -1 : 1;
  }
  if (a.length !== b.length) return a.length < b.length ? -1 : 1;
  for (let i = 0; i < n; i++) if (a[i] !== b[i]) return a.charCodeAt(i) >= 97 ? -1 : 1;
  return 0;
}

const asData = d => (d instanceof Uint8Array ? d : Uint8Array.from(d ?? []));

/**
 * The legacy message of a transaction (the bytes every signer signs), exactly
 * as web3.js 1.99 compiles and serializes it: accounts deduplicated (signer
 * and writable flags merged), then signers before non-signers, writable
 * before read-only, then by key (`compareKeys`); the fee payer first.
 */
export function compileMessage({ feePayer, recentBlockhash, instructions }) {
  if (!recentBlockhash) throw new Error('Transaction recentBlockhash required');
  if (!feePayer) throw new Error('Transaction fee payer required');
  const payer = pubkeyString(feePayer);
  const ixs = instructions.map(ix => ({
    programId: pubkeyString(ix.programId),
    keys: ix.keys.map(k => ({ pubkey: pubkeyString(k.pubkey), isSigner: !!k.isSigner, isWritable: !!k.isWritable })),
    data: asData(ix.data),
  }));
  const metas = [];
  const programs = [];
  for (const ix of ixs) {
    for (const k of ix.keys) metas.push({ ...k });
    if (!programs.includes(ix.programId)) programs.push(ix.programId);
  }
  for (const p of programs) metas.push({ pubkey: p, isSigner: false, isWritable: false });
  const unique = [];
  for (const m of metas) {
    const u = unique.find(x => x.pubkey === m.pubkey);
    if (u) { u.isWritable ||= m.isWritable; u.isSigner ||= m.isSigner; } else unique.push(m);
  }
  unique.sort((x, y) => (x.isSigner !== y.isSigner ? (x.isSigner ? -1 : 1) : x.isWritable !== y.isWritable ? (x.isWritable ? -1 : 1) : compareKeys(x.pubkey, y.pubkey)));
  const at = unique.findIndex(x => x.pubkey === payer);
  if (at > -1) unique.splice(at, 1);
  unique.unshift({ pubkey: payer, isSigner: true, isWritable: true });

  const header = [0, 0, 0];
  for (const m of unique) {
    if (m.isSigner) { header[0]++; if (!m.isWritable) header[1]++; } else if (!m.isWritable) header[2]++;
  }
  const keys = unique.map(m => m.pubkey);
  const index = k => keys.indexOf(k);
  const body = ixs.map(ix => concat([index(ix.programId)], shortvec(ix.keys.length), ix.keys.map(k => index(k.pubkey)), shortvec(ix.data.length), ix.data));
  return concat(header, shortvec(keys.length), ...keys.map(pubkeyBytes), pubkeyBytes(recentBlockhash), shortvec(ixs.length), ...body);
}

/**
 * Parse a legacy message. Refuses (throws) anything the runtime would not
 * sanitize: bad header counts, indexes out of range, a program at index 0,
 * duplicate keys, trailing bytes; and versioned messages.
 * @returns {{header: {numRequiredSignatures, numReadonlySignedAccounts, numReadonlyUnsignedAccounts},
 *   accountKeys: string[], signers: string[], recentBlockhash: string,
 *   instructions: {programIdIndex, programId, accounts: number[], keys: {pubkey, isSigner, isWritable}[], data: Uint8Array}[]}}
 */
export function parseMessage(message) {
  const b = message instanceof Uint8Array ? message : Uint8Array.from(message);
  if (b.length < 3) throw new Error('message: truncated header');
  if (b[0] & 0x80) throw new Error('message: versioned messages are not supported');
  const header = { numRequiredSignatures: b[0], numReadonlySignedAccounts: b[1], numReadonlyUnsignedAccounts: b[2] };
  let [n, o] = readShortvec(b, 3);
  const need = len => { if (o + len > b.length) throw new Error('message: truncated'); };
  need(32 * n);
  const accountKeys = [];
  for (let i = 0; i < n; i++, o += 32) accountKeys.push(toBase58(b.subarray(o, o + 32)));
  if (new Set(accountKeys).size !== n) throw new Error('message: duplicate account keys');
  const { numRequiredSignatures: s, numReadonlySignedAccounts: rs, numReadonlyUnsignedAccounts: ru } = header;
  if (s < 1 || rs >= s || s + ru > n) throw new Error('message: bad header');
  need(32);
  const recentBlockhash = toBase58(b.subarray(o, o + 32));
  o += 32;
  const isWritable = i => (i < s ? i < s - rs : i - s < n - s - ru);
  let count;
  [count, o] = readShortvec(b, o);
  const instructions = [];
  for (let k = 0; k < count; k++) {
    need(1);
    const programIdIndex = b[o++];
    if (programIdIndex === 0 || programIdIndex >= n) throw new Error('message: bad program index');
    let len;
    [len, o] = readShortvec(b, o);
    need(len);
    const accounts = Array.from(b.subarray(o, o + len));
    o += len;
    if (accounts.some(i => i >= n)) throw new Error('message: bad account index');
    [len, o] = readShortvec(b, o);
    need(len);
    const data = b.slice(o, o + len);
    o += len;
    instructions.push({ programIdIndex, programId: accountKeys[programIdIndex], accounts,
      keys: accounts.map(i => ({ pubkey: accountKeys[i], isSigner: i < s, isWritable: isWritable(i) })), data });
  }
  if (o !== b.length) throw new Error('message: trailing bytes');
  return { header, accountKeys, signers: accountKeys.slice(0, s), recentBlockhash, instructions };
}

/**
 * A wire transaction: the signature count, the signatures (64 bytes each, in
 * signer order; a missing one is 64 zero bytes) and the message.
 * `signatures` is an array in signer order or an object {base58 signer: sig}.
 */
export function wireTransaction(message, signatures = []) {
  const msg = message instanceof Uint8Array ? message : Uint8Array.from(message);
  const { signers } = parseMessage(msg);
  let list;
  if (Array.isArray(signatures)) {
    if (signatures.length > signers.length) throw new Error(`${signatures.length} signatures for ${signers.length} signers`);
    list = signers.map((_, i) => signatures[i] ?? null);
  } else {
    for (const k of Object.keys(signatures)) if (!signers.includes(k)) throw new Error(`${k} is not a signer of this message`);
    list = signers.map(k => signatures[k] ?? null);
  }
  const sigs = list.map(sig => {
    if (sig === null) return new Uint8Array(64);
    const x = Uint8Array.from(sig);
    if (x.length !== 64) throw new Error('a signature is 64 bytes');
    return x;
  });
  return concat(shortvec(sigs.length), ...sigs, msg);
}

/** The message bytes of a wire transaction (what its signers signed). */
export function messageOf(wire) {
  const b = wire instanceof Uint8Array ? wire : Uint8Array.from(wire);
  const [count, o] = readShortvec(b, 0);
  if (o + 64 * count > b.length) throw new Error('transaction: truncated signatures');
  return b.slice(o + 64 * count);
}

/**
 * Parse a legacy wire transaction: `parseMessage` of its message, plus
 * `signatures` (64 bytes each, in signer order; all zeros = not signed) and
 * `message` (the signed bytes). Throws unless there is exactly one signature
 * per required signer.
 */
export function parseTransaction(wire) {
  const b = wire instanceof Uint8Array ? wire : Uint8Array.from(wire);
  const [count, o] = readShortvec(b, 0);
  const message = messageOf(b);
  const parsed = parseMessage(message);
  if (count !== parsed.header.numRequiredSignatures) throw new Error(`transaction: ${count} signatures for ${parsed.header.numRequiredSignatures} signers`);
  const signatures = Array.from({ length: count }, (_, i) => b.slice(o + 64 * i, o + 64 * (i + 1)));
  return { ...parsed, signatures, message };
}
