// The relay's payer pool (contract §8.3): 150 fee payers derived from one
// master seed with the keeper's scheme (fclient `payers`, I-49),
//
//   payer_i = ed25519(sha256("PS-FRONTIER-PAYER-v1" ‖ master_seed ‖ pool_id ‖ le32(i))),  pool_id = "relay",
//
// so nothing is lost on a restart. GET /f/relay draws the fee payer
// uniformly at random (OS CSPRNG, no round-robin) among the payers whose
// last known balance is above the floor, so the account a player's
// transaction will lock is not predictable. The pool's total feeds the
// FundsGuard (503 OperatorLowFunds).
import { createHash, randomInt } from 'node:crypto';
import { Keypair, PublicKey } from '@solana/web3.js';

export const PAYER_DOMAIN = 'PS-FRONTIER-PAYER-v1';
export const RELAY_POOL_ID = 'relay';
export const RELAY_POOL_MIN = 150;

/** The 32-byte ed25519 seed of payer `i` of `poolId`. */
export function payerSeed(masterSeed, poolId, i) {
  const le = Buffer.alloc(4);
  le.writeUInt32LE(i);
  return createHash('sha256').update(PAYER_DOMAIN).update(Buffer.from(masterSeed)).update(poolId, 'utf8').update(le).digest();
}

/** Payer `i` of `poolId` as a web3.js Keypair. */
export const derivePayer = (masterSeed, poolId, i) => Keypair.fromSeed(payerSeed(masterSeed, poolId, i));

/** Max accounts per getMultipleAccountsInfo call. */
const MULTI = 100;

export class PayerPool {
  /**
   * `n` payers of `poolId` from `masterSeed` (32 bytes); fewer than 150 only
   * with `dev`. `draw` picks an index in [0, n) (tests pass their own).
   */
  constructor({ masterSeed, n = RELAY_POOL_MIN, poolId = RELAY_POOL_ID, dev = false, minLamports = 0, randomIndex = randomInt }) {
    if (!dev && n < RELAY_POOL_MIN) throw new Error(`the relay pool has at least ${RELAY_POOL_MIN} payers (got ${n})`);
    if (Buffer.from(masterSeed).length !== 32) throw new Error('the master seed is 32 bytes');
    this.poolId = poolId;
    this.keys = Array.from({ length: n }, (_, i) => derivePayer(masterSeed, poolId, i));
    this.index = new Map(this.keys.map((k, i) => [k.publicKey.toBase58(), i]));
    this.minLamports = minLamports;
    this.randomIndex = randomIndex;
    /** Last known balances (lamports; null = unknown). */
    this.balances = this.keys.map(() => null);
    this.refreshedAt = null;
  }

  get size() { return this.keys.length; }
  has(pubkey) { return this.index.has(String(pubkey)); }
  keypair(pubkey) { const i = this.index.get(String(pubkey)); return i === undefined ? null : this.keys[i]; }
  publicKeys() { return this.keys.map(k => k.publicKey.toBase58()); }

  /** The payers a draw may pick: a known balance at or above the floor, or unknown. */
  eligible() {
    return this.keys.map((_, i) => i).filter(i => this.balances[i] === null || this.balances[i] >= this.minLamports);
  }

  /** A fee payer drawn uniformly among the eligible ones (all of them if none is). */
  draw() {
    const e = this.eligible();
    const from = e.length ? e : this.keys.map((_, i) => i);
    return this.keys[from[this.randomIndex(from.length)]];
  }

  /** Read every payer's balance (`connection.getMultipleAccountsInfo`); returns the total. */
  async refresh(connection, now = Date.now()) {
    for (let o = 0; o < this.keys.length; o += MULTI) {
      const slice = this.keys.slice(o, o + MULTI).map(k => new PublicKey(k.publicKey.toBase58()));
      const infos = await connection.getMultipleAccountsInfo(slice, 'confirmed');
      infos.forEach((a, j) => { this.balances[o + j] = a ? Number(a.lamports) : 0; });
    }
    this.refreshedAt = now;
    return this.total();
  }

  total() { return this.balances.reduce((s, b) => s + (b ?? 0), 0); }

  /** Record a payer's balance seen elsewhere (a simulation's post-state). */
  note(pubkey, lamports) {
    const i = this.index.get(String(pubkey));
    if (i !== undefined) this.balances[i] = Number(lamports);
  }
}
