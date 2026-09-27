// What the relay reads from the chain, and how it simulates: the Season
// (its fees, tip floor and join gate), the Clock sysvar (game time: quotas
// reset at the game midnight, never the wall clock's), a Citizen (the
// FileTicket escrow shortfall), and a simulation that returns the fee
// payer's post-balance for the drain guard (§8.3, I-51). Every refusal a
// simulation gives becomes a RouteError named by the program's own error
// table (sdk frontier/codec.mjs), and nothing is charged for it.
import { PublicKey, VersionedTransaction } from '@solana/web3.js';
import { equal } from '../../client/src/bytes.mjs';
import { CLOCK_SYSVAR } from '../../client/src/frontier/addresses.mjs';
import { decodeAccount, programError } from '../../client/src/frontier/codec.mjs';
import { RouteError } from '../routes/errors.mjs';

/** Program errors by the status the relay answers them with (the rest: 400). */
export const FRONTIER_ERROR_STATUS = Object.freeze({
  // Not now (the state or the window): the client may try again later.
  WrongStatus: 409, WindowClosed: 409, TooEarly: 409, LatchClosed: 409, NotFinal: 409, NotResident: 409, HostBusy: 409, HostInTransit: 409,
  Cooldown: 409, DepartureUnsettled: 409, SeedNotReady: 409, OutOfOrder: 409, TicketState: 409, Capacity: 409, SiteTaken: 409, CohortFull: 409,
  SessionExpired: 401, AlreadyDone: 409,
  // The on-chain action bucket (30/h, burst 60).
  Bucket: 429,
  // The signer may not do this.
  Auth: 403, NotOwner: 403, JoinGate: 403,
});

const SYSTEM_PROGRAM_ID = '11111111111111111111111111111111';

/**
 * The RouteError for a failed simulation: `{err, logs}` (send.mjs
 * SimulationError or an RPC value). A `Custom` code is a Frontier code only
 * when the first program that failed (its `Program <id> failed` log line)
 * is the Frontier program `programId`: a System-program failure inside a
 * CPI (a relay payer short of lamports: Custom 1) is 503 OperatorLowFunds,
 * any other program's is ProgramError (integ-W2 review of W2-D).
 */
export function frontierRefusal(e, programId = null) {
  const logs = (e.logs ?? []).slice(-8);
  if (e.err === 'BlockhashNotFound') return new RouteError(409, 'the transaction\'s blockhash expired; build it again with a fresh one', 'BlockhashExpired', { logs });
  if (e.err === 'AlreadyProcessed') return new RouteError(409, 'this transaction was sent already', 'AlreadyProcessed', { logs });
  if (e.err === 'InsufficientFundsForFee' || e.err === 'InsufficientFundsForRent') return new RouteError(503, 'the relay payer cannot pay this now; try again', 'OperatorLowFunds', { logs });
  const failed = (e.logs ?? []).map(l => /^Program (\S+) failed: (.*)$/.exec(l)).find(Boolean);
  if (failed && failed[1] === SYSTEM_PROGRAM_ID) {
    return new RouteError(503, `the relay payer cannot pay this now (system program: ${failed[2]}); try again`, 'OperatorLowFunds', { logs });
  }
  if (failed && programId && failed[1] !== programId) {
    return new RouteError(400, `the transaction would fail in ${failed[1]}: ${failed[2]}`, 'ProgramError', { logs });
  }
  const pe = programError(e.err);
  if (pe?.name) return new RouteError(FRONTIER_ERROR_STATUS[pe.name] ?? 400, `the transaction would fail: ${pe.name} (${pe.code})`, pe.name, { programCode: pe.code, logs });
  if (typeof e.err === 'string') return new RouteError(400, `the transaction would fail: ${e.err}`, e.err, { logs });
  return new RouteError(400, `the transaction would fail: ${JSON.stringify(e.err)}`, 'SimulationFailed', { logs });
}

/**
 * Simulate exactly `wire` (signatures checked, its own blockhash) and read
 * `watch` (base58 keys) after it: `{logs, unitsConsumed, post: [lamports|null]}`.
 * Throws the refusal when it would fail.
 */
export async function simulateWatching(connection, wire, watch, programId = null) {
  const vtx = VersionedTransaction.deserialize(wire);
  if (!equal(vtx.serialize(), wire)) throw new RouteError(400, 'the transaction is not in canonical form', 'InvalidTransaction');
  const r = await connection.simulateTransaction(vtx, {
    sigVerify: true, replaceRecentBlockhash: false, commitment: 'confirmed', accounts: { encoding: 'base64', addresses: watch },
  });
  const v = r?.value ?? {};
  if (v.err) throw frontierRefusal({ err: v.err, logs: v.logs }, programId);
  return { logs: v.logs ?? [], unitsConsumed: v.unitsConsumed ?? null, post: (v.accounts ?? []).map(a => (a ? BigInt(a.lamports) : null)) };
}

/** The Clock sysvar's `{slot, unixTimestamp}` (its account: slot u64, epoch start i64, epoch u64, leader epoch u64, unix time i64). */
export function decodeClock(data) {
  const b = Buffer.from(data);
  if (b.length < 40) throw new Error('clock sysvar: short');
  return { slot: b.readBigUInt64LE(0), unixTimestamp: Number(b.readBigInt64LE(32)) };
}

/** Cached reads of the season's accounts and the Clock (TTLs in ms). */
export class ChainView {
  constructor({ connection, addresses, seasonTtlMs = 5_000, clockTtlMs = 1_000, now = Date.now }) {
    Object.assign(this, { connection, addresses, seasonTtlMs, clockTtlMs, now });
    this.cache = new Map();
  }

  async cached(key, ttl, load) {
    const hit = this.cache.get(key);
    if (hit && this.now() - hit.at < ttl) return hit.value;
    const value = await load();
    this.cache.set(key, { at: this.now(), value });
    return value;
  }

  /** The decoded Season, or 503 WorldUnavailable while it does not exist. */
  season() {
    return this.cached('season', this.seasonTtlMs, async () => {
      const a = await this.connection.getAccountInfo(new PublicKey(this.addresses.season), 'confirmed');
      if (!a) throw new RouteError(503, 'the season does not exist (yet)', 'WorldUnavailable');
      try {
        return decodeAccount('Season', a.data, { seasonId: this.addresses.seasonId });
      } catch (e) {
        throw new RouteError(503, `the season account is not readable: ${e.message}`, 'WorldUnavailable');
      }
    });
  }

  /** The chain's Clock (game time). */
  clock() {
    return this.cached('clock', this.clockTtlMs, async () => {
      const a = await this.connection.getAccountInfo(new PublicKey(CLOCK_SYSVAR), 'confirmed');
      if (a) return decodeClock(a.data);
      const slot = await this.connection.getSlot('confirmed');
      return { slot: BigInt(slot), unixTimestamp: Number(await this.connection.getBlockTime(slot)) };
    });
  }

  /** A decoded Citizen of this season, or null (absent or not one). Not cached: the escrow must be current. */
  async citizen(address) {
    const a = await this.connection.getAccountInfo(new PublicKey(address), 'confirmed');
    if (!a || a.data.length === 0) return null;
    try { return decodeAccount('Citizen', a.data, { seasonId: this.addresses.seasonId }); } catch { return null; }
  }
}
