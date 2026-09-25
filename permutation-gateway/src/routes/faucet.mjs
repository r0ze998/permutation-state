// POST /faucet {owner}  (localnet and devnet; never mainnet) → a USDC account
//                       with 100 test USDC of this gateway's own mint (no value).
//                       One request per owner per 10 minutes, at most 60 per hour.
import { PublicKey } from '@solana/web3.js';
import { createTokenAccountIxs, mintToIx } from '../spl.mjs';
import { send } from '../send.mjs';
import { RouteError } from './errors.mjs';

export const FAUCET_AMOUNT = 100_000_000n; // 100 test USDC (6 decimals)

/**
 * Faucet rate limits: one grant per owner per `cooldownMs`, and at most
 * `perHour` grants in any hour over all owners. A request takes a slot
 * before minting (so concurrent requests cannot both pass) and gives it back
 * if the mint fails.
 */
export class FaucetLimiter {
  constructor({ cooldownMs = 10 * 60_000, perHour = 60, now = Date.now } = {}) {
    Object.assign(this, { cooldownMs, perHour, now });
    this.lastByOwner = new Map(); // owner → time of its last grant
    this.grants = []; // times of the grants of the last hour, oldest first
  }

  /** Take a slot for `owner`: returns a release function, or throws a 429 RouteError. */
  acquire(owner) {
    const t = this.now();
    while (this.grants.length && t - this.grants[0] >= 3600_000) this.grants.shift();
    for (const [k, at] of this.lastByOwner) if (t - at >= this.cooldownMs) this.lastByOwner.delete(k);
    if (this.lastByOwner.has(owner)) throw new RouteError(429, `one faucet request per owner every ${this.cooldownMs / 60_000} minutes`, 'FaucetCooldown');
    if (this.grants.length >= this.perHour) throw new RouteError(429, 'the faucet is busy; try again later', 'FaucetBusy');
    this.lastByOwner.set(owner, t);
    this.grants.push(t);
    let released = false;
    return () => {
      if (released) return;
      released = true;
      if (this.lastByOwner.get(owner) === t) this.lastByOwner.delete(owner);
      const i = this.grants.indexOf(t);
      if (i >= 0) this.grants.splice(i, 1);
    };
  }
}

export const faucetRoutes = {
  'POST /faucet': async ({ base, cfg, store, keys, faucet }, req) => {
    // The mint is this gateway's own test token; on mainnet there is no faucet.
    if (cfg.cluster !== 'localnet' && cfg.cluster !== 'devnet') throw new RouteError(403, 'the faucet is for localnet and devnet only', 'FaucetDisabled');
    const { owner } = await req.json();
    let ownerKey;
    try { ownerKey = new PublicKey(owner); } catch { throw new RouteError(400, 'owner must be a public key', 'InvalidOwner'); }
    const release = faucet.acquire(ownerKey.toBase58());
    try {
      const admin = keys('admin'), crank = keys('crank');
      const account = keys(`faucet-${ownerKey.toBase58().slice(0, 16)}`);
      const mint = new PublicKey(store.state.mint);
      if (!(await base.getAccountInfo(account.publicKey))) {
        await send(base, await createTokenAccountIxs(base, { payer: crank.publicKey, account: account.publicKey, mint, owner: ownerKey }), [crank, account], 'faucet account');
      }
      await send(base, [mintToIx({ mint, dest: account.publicKey, authority: admin.publicKey, amount: FAUCET_AMOUNT })], [crank, admin], 'faucet mint');
      return { body: { usdcAccount: account.publicKey.toBase58(), mint: store.state.mint, amount: FAUCET_AMOUNT.toString(), note: `${cfg.cluster} test USDC of this gateway's own mint, no value` } };
    } catch (e) {
      release();
      throw e;
    }
  },
};
