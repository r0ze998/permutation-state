// How the gateway answers failures: route errors carry their own status;
// program errors are mapped by class (a late submission is 409 so clients
// retry next tick, a refused signer 403, a bad request 400); anything else
// (RPC trouble, a confirmation that timed out) is 500.
import { chainError } from '../../client/src/codec.mjs';

/** A failure a route reports on purpose: HTTP status, message, machine-readable code. */
export class RouteError extends Error {
  constructor(status, message, code = null, extra = {}) {
    super(message);
    this.status = status;
    this.code = code;
    this.extra = extra;
  }
}

/** Program errors by the status the gateway answers them with. */
export const CHAIN_ERROR_STATUS = Object.freeze({
  // Too late for this tick: the client retries on the next one.
  TickFrozen: 409, WrongTick: 409,
  // The season or account is not in a state that allows it (now, or ever).
  AlreadyInitialized: 409, WrongStatus: 409, SeasonFull: 409, TooEarly: 409, AlreadyClaimed: 409, NothingToClaim: 409,
  SeasonNotOver: 409, InboxFull: 409, InputNotPublished: 409,
  // The signer may not do this.
  MissingSignature: 403, Unauthorized: 403, WrongOffice: 403,
  // The request itself is wrong.
  InvalidInstruction: 400, WrongPda: 400, NotInitialized: 400, InvalidName: 400, WrongMint: 400, WrongTokenAccount: 400, Rules: 400,
  OverBudget: 400, MissingNation: 400, InvalidParams: 400, WrongWorld: 400,
  // The gateway or the program is misconfigured.
  WorldTooSmall: 500, WrongDelegationProgram: 500, WrongMagicProgram: 500,
});

/** Status for a program error name (an unknown `Custom(n)` is the program refusing the request: 400). */
export const chainErrorStatus = code => CHAIN_ERROR_STATUS[code] ?? (code ? 400 : 500);

/** The response for a thrown error, and whether it is worth logging. */
export function errorResponse(e) {
  if (e instanceof RouteError) return { status: e.status, body: { error: e.message, code: e.code, ...e.extra }, log: e.status >= 500 };
  const code = chainError(e.message);
  const status = chainErrorStatus(code);
  return {
    status,
    body: { error: code ? `${code}: ${e.message}` : e.message, code, logs: e.logs?.slice(-6) },
    log: !(code === 'TickFrozen' || code === 'WrongTick'),
  };
}
