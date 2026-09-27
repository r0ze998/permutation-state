// Read routes of the relay (contract §8.3):
//
//   GET /f/season              {programId, season, seasonId, cluster, relayPool, quotas, heraldUrl, joinGate, inviteRequired, tipMin, tipPresets, marchFee, sealBond}
//   GET /f/quota?citizen=      {left, resetsAt, lamportsLeft}   (citizen: the Citizen account's address)
//   GET /f/tx/{signature}      {state: "landed|failed|expired|unknown", slot?, code?}
import { encode as base58, decode as fromBase58 } from '../../../client/src/base58.mjs';
import { programError } from '../../../client/src/frontier/codec.mjs';
import { seasonTipMin, tipPresets } from '../../../client/src/frontier/fees.mjs';
import { RouteError } from '../../routes/errors.mjs';
import { dailyTxs, gameDay, QUOTA } from '../quota.mjs';
import { lamportsPerDay } from '../shapes.mjs';

const isKey = (s, n = 32) => {
  try { return typeof s === 'string' && fromBase58(s).length === n; } catch { return false; }
};

export const seasonRoutes = {
  'GET /f/season': async ctx => {
    const s = await ctx.chain.season();
    const gated = s.JOIN_GATE.some(x => x !== 0);
    const tipMin = seasonTipMin(s);
    return {
      body: {
        programId: ctx.programId, season: ctx.addresses.season, seasonId: ctx.addresses.seasonId.toString(), cluster: ctx.cfg.cluster,
        relayPool: ctx.pool.size,
        quotas: { txsPerDay: dailyTxs(0), txsPerDayAfter: dailyTxs(QUOTA.earlyDays), afterDay: QUOTA.earlyDays, burst: QUOTA.burst, departsPerDay: QUOTA.departsPerDay,
          lamportsPerDay: lamportsPerDay(s).toString() },
        heraldUrl: ctx.cfg.heraldUrl, joinGate: gated ? base58(s.JOIN_GATE) : null, inviteRequired: gated,
        tipMin: tipMin.toString(), tipPresets: tipPresets(tipMin).map(String), marchFee: s.MARCH_FEE.toString(), sealBond: s.SEAL_BOND.toString(),
      },
    };
  },

  'GET /f/quota': async (ctx, req) => {
    const citizen = req.url.searchParams.get('citizen');
    if (!isKey(citizen)) throw new RouteError(400, 'citizen must be the Citizen account\'s address (base58)', 'BadRequest');
    const s = await ctx.chain.season();
    const clock = await ctx.chain.clock();
    const day = gameDay(clock.unixTimestamp, s.GENESIS_TS);
    return { body: ctx.quota.status(`citizen:${citizen}`, { day, lamportsCap: lamportsPerDay(s), genesisTs: Number(s.GENESIS_TS) }) };
  },

  'GET /f/tx/': async (ctx, req) => {
    const sig = req.url.pathname.slice('/f/tx/'.length);
    if (!isKey(sig, 64)) throw new RouteError(400, 'a transaction signature (base58, 64 bytes) is required', 'BadRequest');
    const r = await ctx.connection.getSignatureStatuses([sig], { searchTransactionHistory: true });
    const st = r?.value?.[0] ?? null;
    if (st) {
      if (st.err) {
        const pe = programError(st.err);
        return { body: { state: 'failed', slot: st.slot, code: pe?.name ?? (typeof st.err === 'string' ? st.err : 'TransactionFailed'), programCode: pe?.code } };
      }
      if (st.confirmationStatus === 'confirmed' || st.confirmationStatus === 'finalized') return { body: { state: 'landed', slot: st.slot } };
    }
    const known = ctx.sent.get(sig);
    if (known?.lastValidBlockHeight && (await ctx.connection.getBlockHeight('confirmed')) > known.lastValidBlockHeight) return { body: { state: 'expired' } };
    return { body: { state: 'unknown' } };
  },
};
