// Test USDC and token balances.
//
//   POST /faucet {owner}  localnet and devnet only (never mainnet), while registration is open:
//                         exactly the entry fee plus the default deposit of this gateway's
//                         own test mint (no value) into the owner's associated token account,
//                         once per owner per season (src/faucet.mjs; the operator's AI members
//                         are funded by the same function)
//                         → {usdcAccount, mint, amount, note}; an owner funded before gets its
//                         account and amount "0" (note "already funded")
//                         403 FaucetDisabled, 400 InvalidOwner, 409 RegistrationClosed,
//                         409 SeasonFull (as many owners funded and not registered yet as
//                         there are seats left)
//   GET  /usdc?owner=     the owner's token accounts of the season's mint
//                         → {mint, decimals, accounts: [{address, amount}]} (largest first; cached 5 s,
//                         and refreshed by a POST /faucet for that owner)
import { PublicKey } from '@solana/web3.js';
import { MAX_MEMBERS } from '../../client/src/codec.mjs';
import { faucetAvailable, fundOwner, outstandingGrants, registrationCost } from '../faucet.mjs';
import { pendingAi, registrationOf, registrationOpen } from '../season.mjs';
import { tokenAccountsOf } from '../spl.mjs';
import { RouteError, routeSeason } from './errors.mjs';

/** `owner` (base58) as a PublicKey, or 400 InvalidOwner (`what`: the parameter's name). */
export const ownerKey = (owner, what = 'owner') => {
  try {
    if (typeof owner !== 'string' || !owner) throw new Error('missing');
    return new PublicKey(owner);
  } catch {
    throw new RouteError(400, `${what} must be a public key`, 'InvalidOwner');
  }
};

export const faucetRoutes = {
  'POST /faucet': async ({ base, cfg, chain, crank, store, keys, registry, now, usdc }, req) => {
    // The mint is this gateway's own test token; on mainnet there is no faucet.
    if (!faucetAvailable(cfg.cluster)) throw new RouteError(403, 'the faucet is for localnet and devnet only', 'FaucetDisabled');
    const { owner } = await req.json();
    const key = ownerKey(owner);
    const season = await routeSeason({ base, chain });
    const state = store.state;
    if (!registrationOpen({ state, season, phase: crank.phase, now: now() })) throw new RouteError(409, 'the faucet is open while registration is', 'RegistrationClosed');
    const note = `${cfg.cluster} test USDC of this gateway's own mint, no value`;
    // The owner's balance changes (or is about to be read after a grant):
    // the next GET /usdc reads it again instead of the cached one.
    usdc?.delete(key.toBase58());
    const had = state.faucet?.[key.toBase58()];
    if (had) return { body: { usdcAccount: had.account, mint: state.mint, amount: '0', note: 'already funded' } };
    // Never more owners waiting to register than seats left for them.
    const wallets = (await registry.list()).map(m => m.wallet);
    if (outstandingGrants(state, wallets) >= MAX_MEMBERS - season.memberCount - pendingAi(state)) throw new RouteError(409, 'the season is full', 'SeasonFull');
    const reg = registrationOf(state, { entryFee: season.entryFee });
    const r = await fundOwner({ base, store, owner: key, amount: registrationCost(season.entryFee, reg.deposit), keys });
    usdc?.delete(key.toBase58());
    return { body: { usdcAccount: r.account.toBase58(), mint: state.mint, amount: r.amount.toString(), note: r.already ? 'already funded' : note } };
  },

  'GET /usdc': async ({ base, store, usdc }, req) => {
    const owner = ownerKey(req.url.searchParams.get('owner'));
    const mint = store.state.mint;
    const accounts = await usdc.get(owner.toBase58(), () => tokenAccountsOf(base, owner, mint));
    // This gateway's own test mint, created with 6 decimals (spl.mjs createMintIxs), like USDC.
    return { body: { mint, decimals: 6, accounts: accounts.map(a => ({ address: a.address, amount: a.amount.toString() })) } };
  },
};
