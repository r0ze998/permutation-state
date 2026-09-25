// Sending transactions with clear failures: every error carries the program
// logs, and successful sends return the logs so replay records
// (`Program data:` PS_TICK / PS_GENESIS …) can be archived.
import { Transaction } from '@solana/web3.js';
import { parseRecord, RECORD_TAGS } from '../client/src/codec.mjs';
import { isTransientRpcError, poll, retry, sleep } from '../client/src/retry.mjs';

export class SendError extends Error {
  constructor(label, cause, logs) {
    super(`${label}: ${cause}`);
    this.label = label;
    this.logs = logs || [];
  }
}

const TX_OPTS = { commitment: 'confirmed', maxSupportedTransactionVersion: 0 };

/**
 * Build, sign and send `ixs`, and wait for confirmation. Returns
 * { signature, logs, cu, records, fetched }.
 *
 * The fee payer is `signers[0]` unless `feePayer` (a PublicKey) names
 * another; either way it must be one of `signers`.
 */
export async function send(connection, ixs, signers, label, { feePayer = signers[0]?.publicKey } = {}) {
  if (!feePayer || !signers.some(s => s.publicKey.equals(feePayer))) throw new SendError(label, 'the fee payer must be one of the signers');
  const tx = new Transaction().add(...ixs);
  tx.feePayer = feePayer;
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash('confirmed');
  tx.recentBlockhash = blockhash;
  tx.sign(...signers);
  return submit(connection, tx.serialize(), label, lastValidBlockHeight);
}

/**
 * Send an already signed transaction (e.g. relayed for a member). Pass the
 * `lastValidBlockHeight` of its blockhash so that an expired one fails fast
 * instead of waiting out the confirmation timeout.
 */
export async function sendSigned(connection, tx, label, { lastValidBlockHeight } = {}) {
  return submit(connection, tx.serialize(), label, lastValidBlockHeight);
}

async function submit(connection, bytes, label, lastValidBlockHeight) {
  let signature;
  try {
    signature = await sendRaw(connection, bytes);
    await confirm(connection, signature, lastValidBlockHeight);
  } catch (e) {
    const t = signature ? await connection.getTransaction(signature, TX_OPTS).catch(() => null) : null;
    throw new SendError(label, e.message || String(e), t?.meta?.logMessages || e.logs);
  }
  return fetchResult(connection, signature);
}

/**
 * Send raw bytes, retrying transient RPC errors (public RPCs answer 429 under
 * load). Resending the same signed bytes is idempotent: one signature.
 */
function sendRaw(connection, bytes) {
  return retry(() => connection.sendRawTransaction(bytes, { skipPreflight: true }),
    { attempts: 6, delayMs: 500, backoff: 2, retryIf: isTransientRpcError });
}

/**
 * Poll signature status (HTTP only). Websocket confirmation is not relied on:
 * against a local ER it can stall for its full timeout.
 */
export async function confirm(connection, signature, lastValidBlockHeight, timeoutMs = 60_000) {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    // A failed poll (e.g. HTTP 429 from a public RPC) is not a failed transaction: poll again.
    const st = await connection.getSignatureStatuses([signature]).then(r => r.value[0], () => undefined);
    if (st?.err) throw new Error(JSON.stringify(st.err));
    if (st && (st.confirmationStatus === 'confirmed' || st.confirmationStatus === 'finalized')) return;
    if (lastValidBlockHeight && (await connection.getBlockHeight('confirmed').catch(() => 0)) > lastValidBlockHeight) throw new Error('blockhash expired');
    await sleep(st === undefined ? 600 : 150);
  }
  throw new Error('confirmation timed out');
}

async function fetchResult(connection, signature) {
  // A confirmed transaction can take a moment to become fetchable (the ER
  // indexes its ledger asynchronously under load); callers that archive
  // records need its logs, so wait up to ~15 s rather than return none.
  const t = await poll(() => connection.getTransaction(signature, TX_OPTS), { attempts: 60, delayMs: 250 });
  const logs = t?.meta?.logMessages || [];
  return { signature, logs, cu: t?.meta?.computeUnitsConsumed ?? null, records: records(logs), fetched: !!t };
}

/** Decode `Program data:` log lines (sol_log_data: base64 fields separated by spaces) into the program's records. */
export function records(logs) {
  return logs
    .filter(l => l.startsWith('Program data: '))
    .map(l => l.slice('Program data: '.length).split(' ').map(f => new Uint8Array(Buffer.from(f, 'base64'))))
    .map(parseRecord)
    .filter(r => RECORD_TAGS.includes(r.tag));
}

/**
 * The blockhashes this gateway handed out, with their last valid block
 * height, so that relayed transactions (signed elsewhere against one of
 * them) can be confirmed with an expiry. An unknown blockhash is bounded by
 * the current one's expiry: it cannot outlive a newer blockhash.
 */
export class BlockhashBook {
  constructor(connection, { max = 512 } = {}) {
    this.connection = connection;
    this.max = max;
    this.known = new Map();
  }

  /** The latest blockhash (recorded). */
  async latest() {
    const b = await this.connection.getLatestBlockhash('confirmed');
    this.known.delete(b.blockhash);
    this.known.set(b.blockhash, b.lastValidBlockHeight);
    while (this.known.size > this.max) this.known.delete(this.known.keys().next().value);
    return b;
  }

  /** Last valid block height of `blockhash`; `hint` (from the client) can only lower the bound. */
  async expiryOf(blockhash, hint) {
    const known = this.known.get(blockhash);
    if (known !== undefined) return known;
    const bound = (await this.latest()).lastValidBlockHeight;
    return Number.isSafeInteger(hint) && hint > 0 ? Math.min(hint, bound) : bound;
  }
}
