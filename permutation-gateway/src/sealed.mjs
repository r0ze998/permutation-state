// Sealed orders the gateway holds for the members it hosts (commit–reveal).
//
// An office's orders go on chain in two steps: before the tick's deadline
// only a commitment (`orderCommitment(batch, salt)`, `CommitOrders`); after
// the commitments close (`CloseCommits`) the batch and its salt
// (`RevealOrders`). Until then nobody else — other nations, agents watching
// the chain — can read or react to them (a hosted member's host can: see
// Trust below).
//
// For a hosted member the gateway draws the salt, keeps the plaintext here
// (on disk, written before the commitment is sent, so neither a crash nor a
// lost confirmation loses it), sends the commitment (POST /submit), and the
// crank reveals, once the commitments are closed, the kept batch whose
// commitment is the one on chain.
//
// Trust: a hosted member has handed its session key to this gateway, so
// the gateway can already sign anything for it; it also sees the member's
// orders before the deadline and chooses when to reveal them. Sealing
// protects hosted members from everyone else, not from their host. A member
// that wants no host signs and reveals itself (the SDK's `submit` and
// `revealWhenOpen`, relayed through POST /relay).
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
