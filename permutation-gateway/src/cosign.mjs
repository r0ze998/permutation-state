// Co-signing a member's transaction (the gateway pays the fee): the wire
// bytes are parsed exactly as they were signed (client/src/solana-tx.mjs,
// stricter than the runtime), the route checks their shape, the member's
// signature is verified here, the crank signs the same message, and the
// resulting bytes are simulated with every signature checked
// (send.mjs `simulateWire`) before they are sent — a transaction that would
// fail never costs the crank a fee.
import { encode as base58 } from '../client/src/base58.mjs';
import { fromBase64 } from '../client/src/bytes.mjs';
import { chainError } from '../client/src/codec.mjs';
import { parseTransaction, pubkeyBytes, wireTransaction } from '../client/src/solana-tx.mjs';
import { signTalk as signEd25519, verifyTalk as verifyEd25519 } from '../client/src/talk-node.mjs';
import { TOKEN_PROGRAM } from '../client/src/player.mjs';
import { CHAIN_ERROR_STATUS, RouteError } from './routes/errors.mjs';
import { SimulationError, simulateWire } from './send.mjs';

/** A base64 wire transaction, parsed (`{wire, tx}`), or a 400 with `code`. */
export function parseWire(b64, { code = 'InvalidTransaction', what = 'tx' } = {}) {
  try {
    if (typeof b64 !== 'string' || !b64) throw new Error('missing');
    const wire = fromBase64(b64);
    return { wire, tx: parseTransaction(wire) };
  } catch {
    throw new RouteError(400, `${what} must be a base64 serialized legacy transaction`, code);
  }
}

/** Whether `signer` (base58) signed `tx` (parsed) with a valid ed25519 signature over its message. */
export function signedBy(tx, signer) {
  const i = tx.signers.indexOf(signer);
  return i >= 0 && verifyEd25519(tx.message, tx.signatures[i], pubkeyBytes(signer));
}

/** The wire bytes of `tx` with the crank's signature added (it must be the fee payer, the first signer). */
export function coSign(tx, crank) {
  if (tx.signers[0] !== crank.publicKey.toBase58()) throw new Error('the crank is not the fee payer');
  const signatures = [...tx.signatures];
  signatures[0] = signEd25519(tx.message, crank);
  return wireTransaction(tx.message, signatures);
}

/** A transaction's id: its first (fee payer's) signature, base58. */
export const signatureOf = wire => base58(wire.subarray(1, 65));

/**
 * The refusal for a transaction the simulation said would fail: a program
 * error by its name and class (errors.mjs), a token-program failure inside
 * it (insufficient funds…), an expired blockhash, or the runtime's own error.
 */
export function refusal(e, programId) {
  const logs = e.logs ?? [];
  const body = { logs: logs.slice(-6) };
  if (logs.some(l => /insufficient (funds|lamports)/i.test(l))) return new RouteError(400, 'insufficient funds (USDC) for this transaction', 'InsufficientFunds', body);
  if (e.err === 'BlockhashNotFound') return new RouteError(409, 'the transaction\'s blockhash expired; build it again with a fresh one', 'BlockhashExpired', body);
  if (e.err === 'AlreadyProcessed') return new RouteError(409, 'this transaction was sent already', 'AlreadyProcessed', body);
  if (typeof e.err === 'string') return new RouteError(400, `the transaction would fail: ${e.err}`, e.err, body);
  // The first program that failed is where the error comes from (the
  // program calling it fails with the same code after it).
  const failed = logs.map(l => /^Program (\S+) failed: (.*)$/.exec(l)).find(Boolean);
  if (failed && failed[1] !== programId) {
    return new RouteError(400, `the transaction would fail in ${failed[1] === TOKEN_PROGRAM ? 'the token program' : failed[1]}: ${failed[2]}`, failed[1] === TOKEN_PROGRAM ? 'TokenError' : 'ProgramError', body);
  }
  const code = chainError(JSON.stringify(e.err));
  if (code) return new RouteError(CHAIN_ERROR_STATUS[code] ?? 400, `the transaction would fail: ${code}`, code, body);
  return new RouteError(400, `the transaction would fail: ${e.message}`, 'SimulationFailed', body);
}

/** Simulate the exact `wire` on `connection`; a failure becomes the RouteError for it. */
export async function simulateOrRefuse(connection, wire, programId) {
  try {
    return await simulateWire(connection, wire);
  } catch (e) {
    if (e instanceof SimulationError) throw refusal(e, programId);
    throw e;
  }
}
