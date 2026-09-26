// Test USDC for registering (localnet and devnet): exactly the season's entry
// fee plus the public default deposit, once per owner per season (kept in the
// state file), into the owner's associated token account (created if
// missing). The crank pays the fee and the rent; the admin mints (the
// gateway's own test mint, no value).
//
// People (POST /faucet) and the operator's AI members (the crank, at a time
// of their own before they register) are funded by this one function, so
// their funding transactions have the same shape (V5 §18.2).
import { PublicKey } from '@solana/web3.js';
import { namedKey } from './config.mjs';
import { send } from './send.mjs';
import { associatedTokenAccount, createAtaIdempotentIx, mintToIx } from './spl.mjs';

/** The faucet exists on these clusters only (never on mainnet). */
export const faucetAvailable = cluster => cluster === 'localnet' || cluster === 'devnet';

/** What one registration costs: the entry fee plus the public default deposit (base units). */
export const registrationCost = (entryFee, deposit) => BigInt(entryFee) + BigInt(deposit ?? 0);

// Grants being sent, per season state: a second request for the same owner
// waits for the first instead of minting twice.
const sending = new WeakMap();

/**
 * Fund `owner` (PublicKey) with `amount` test USDC, once per season. Returns
 * `{account, amount, signature, already}`; `already` (amount 0) when the
 * owner was funded before. `ai` marks the operator's own grants in the
 * private state file (they do not count against the people's cap).
 */
export async function fundOwner({ base, store, owner, amount, keys = namedKey, ai = false, label = 'faucet' }) {
  const state = store.state;
  const key = new PublicKey(owner).toBase58();
  state.faucet ??= {};
  const done = state.faucet[key];
  if (done) return { account: new PublicKey(done.account), amount: 0n, signature: done.signature, already: true };
  let inflight = sending.get(state);
  if (!inflight) sending.set(state, (inflight = new Map()));
  if (inflight.has(key)) {
    await inflight.get(key).catch(() => null);
    return fundOwner({ base, store, owner, amount, keys, ai, label });
  }
  const run = (async () => {
    const admin = keys('admin'), crank = keys('crank');
    const mint = new PublicKey(state.mint);
    const account = associatedTokenAccount(owner, mint);
    const r = await send(base, [createAtaIdempotentIx({ payer: crank.publicKey, owner, mint }), mintToIx({ mint, dest: account, authority: admin.publicKey, amount })],
      [crank, admin], label);
    state.faucet[key] = { account: account.toBase58(), amount: amount.toString(), signature: r.signature, at: Date.now(), ...(ai ? { ai: true } : {}) };
    store.save();
    return { account, amount, signature: r.signature, already: false };
  })();
  inflight.set(key, run);
  try {
    return await run;
  } finally {
    inflight.delete(key);
  }
}

/** Owners funded for people (not the operator's AI members) who are not members yet. */
export function outstandingGrants(state, memberWallets) {
  const members = new Set(memberWallets);
  return Object.entries(state.faucet ?? {}).filter(([owner, g]) => !g.ai && !members.has(owner)).length;
}
