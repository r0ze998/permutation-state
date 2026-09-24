// Sending transactions with clear failures: every error carries the program
// logs, and successful sends return the logs so replay records
// (`Program data:` PS_TICK / PS_GENESIS) can be archived.
import { Transaction } from '@solana/web3.js';
import { parseRecord } from '../client/src/codec.mjs';

export class SendError extends Error {
  constructor(label, cause, logs) {
    super(`${label}: ${cause}`);
    this.label = label;
    this.logs = logs || [];
  }
}

/**
 * Send `ixs` signed by `signers` (the first signer pays fees) and wait for
 * confirmation. Returns { signature, logs, cu, records }.
 */
export async function send(connection, ixs, signers, label, { feePayer } = {}) {
  const tx = new Transaction().add(...ixs);
  tx.feePayer = (feePayer || signers[0]).publicKey ?? feePayer;
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash('confirmed');
  tx.recentBlockhash = blockhash;
  tx.sign(...signers);
  let signature;
  try {
    signature = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: true });
    await confirm(connection, signature, lastValidBlockHeight);
  } catch (e) {
    const t = signature ? await connection.getTransaction(signature, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 }).catch(() => null) : null;
    throw new SendError(label, e.message || String(e), t?.meta?.logMessages || e.logs);
  }
  return fetchResult(connection, signature);
}

/** Send an already signed transaction (e.g. relayed for an agent). */
export async function sendSigned(connection, tx, label) {
  const signature = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: true });
  try {
    await confirm(connection, signature);
  } catch (e) {
    const t = await connection.getTransaction(signature, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 }).catch(() => null);
    throw new SendError(label, e.message, t?.meta?.logMessages);
  }
  return fetchResult(connection, signature);
}

/**
 * Poll signature status (HTTP only). Websocket confirmation is not relied on:
 * against a local ER it can stall for its full timeout.
 */
export async function confirm(connection, signature, lastValidBlockHeight, timeoutMs = 60_000) {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    const { value } = await connection.getSignatureStatuses([signature]);
    const st = value[0];
    if (st?.err) throw new Error(JSON.stringify(st.err));
    if (st && (st.confirmationStatus === 'confirmed' || st.confirmationStatus === 'finalized')) return;
    if (lastValidBlockHeight && (await connection.getBlockHeight('confirmed').catch(() => 0)) > lastValidBlockHeight) throw new Error('blockhash expired');
    await new Promise(r => setTimeout(r, 150));
  }
  throw new Error('confirmation timed out');
}

async function fetchResult(connection, signature) {
  let t = null;
  for (let i = 0; i < 20 && !t; i++) {
    t = await connection.getTransaction(signature, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 }).catch(() => null);
    if (!t) await new Promise(r => setTimeout(r, 150));
  }
  const logs = t?.meta?.logMessages || [];
  return { signature, logs, cu: t?.meta?.computeUnitsConsumed ?? null, records: records(logs) };
}

/** Decode `Program data:` log lines (sol_log_data: base64 fields separated by spaces). */
export function records(logs) {
  return logs
    .filter(l => l.startsWith('Program data: '))
    .map(l => l.slice('Program data: '.length).split(' ').map(f => new Uint8Array(Buffer.from(f, 'base64'))))
    .map(parseRecord)
    .filter(r => r.tag === 'PS_TICK' || r.tag === 'PS_GENESIS');
}
