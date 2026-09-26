// Sealed orders the gateway holds until they are revealed (commit–reveal).
//
// An office's orders go on chain in two steps: before the tick's deadline
// only a commitment (`orderCommitment(batch, salt)`, `CommitOrders`); after
// the commitments close (`CloseCommits`) the batch and its salt
// (`RevealOrders`). Until then nobody else — other nations, agents watching
// the chain — can read or react to them (the gateway can: see Trust below).
//
// Batches come here two ways, and are kept on disk before their commitment
// is sent (neither a crash nor a lost confirmation loses them):
// * the operator's AI members (POST /submit): the gateway draws the salt and
//   sends the commitment signed with the AI member's session key;
// * members who sign their own CommitOrders (a browser, an agent) deposit
//   batch and salt first (POST /seal, signed by the office key).
// Once the commitments are closed the crank reveals every kept batch whose
// commitment is the one on chain, signed by the crank alone, the same way
// for everyone.
//
// Trust: whoever deposits here lets the operator (who also runs the hidden
// AI members) read those orders before the deadline, and relies on it to
// reveal them; the program still checks every reveal against the
// commitment, so it cannot change them. A member that deposits nothing
// reveals itself (the SDK's `revealWhenOpen`, relayed through POST /relay).
import { randomBytes } from 'node:crypto';
import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { fromHex, toHex } from '../client/src/bytes.mjs';
import { orderCommitment } from '../client/src/codec.mjs';

/** A new salt and the commitment of `batch` = {civ, tick, role, member, decisionDigest, orders, adopt}. */
export function seal(batch, salt = new Uint8Array(randomBytes(32))) {
  return { salt, commitment: orderCommitment(batch, salt) };
}

export class SealedStore {
  /** @param {string|null} file where the kept batches live (null: memory only, for tests) */
  constructor(file = null) {
    this.file = file;
    this.entries = new Map();
    if (file && existsSync(file)) {
      for (const e of JSON.parse(readFileSync(file, 'utf8'))) this.entries.set(keyOf(e), e);
    }
  }

  /**
   * Keep a sealed batch under its commitment (before sending it). Several
   * may be kept for one office and tick; the one to reveal is the one whose
   * commitment is on chain.
   */
  put(batch, salt, commitment) {
    const e = { ...batch, decisionDigest: toHex(batch.decisionDigest), salt: toHex(salt), adopt: batch.adopt ?? [], commitment: toHex(commitment) };
    this.entries.set(keyOf(e), e);
    this.save();
  }

  /** The kept batches of `tick`, with their salts and commitments, ready for `revealOrders`. */
  forTick(tick) {
    return [...this.entries.values()]
      .filter(e => e.tick === tick)
      .map(e => ({ ...e, decisionDigest: fromHex(e.decisionDigest), salt: fromHex(e.salt), commitment: e.commitment }));
  }

  /** Forget every batch of a tick before `tick` (resolved). */
  dropBefore(tick) {
    let changed = false;
    for (const [k, e] of this.entries) if (e.tick < tick) { this.entries.delete(k); changed = true; }
    if (changed) this.save();
  }

  save() {
    if (!this.file) return;
    const tmp = `${this.file}.tmp`;
    writeFileSync(tmp, JSON.stringify([...this.entries.values()]));
    renameSync(tmp, this.file);
  }
}

const keyOf = e => `${e.tick}:${e.civ}:${e.role}:${e.commitment}`;
