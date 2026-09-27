// The keeper link's public routes (contract §8.3, I-24):
//
//   POST /f/reveal {holding, transit_slot, plain_b64, salt_b64, ct_hash_b64}  → the keeper's POST /v1/reveal, answer passed through
//   POST /f/nudge  {province: [P, Q], bell}                                    → the keeper's POST /v1/nudge
//
// A self-reveal is material, not a transaction: nobody's account is named
// in advance, and the keeper's random reveal payer sends it. Besides the
// per-address limit, reveals are limited per holding (a holding has at most
// four transits, each revealed once or twice).
import { RouteError } from '../../routes/errors.mjs';
import { nudgeBody, revealBody } from '../keeperlink.mjs';

export const PER_HOLDING_REVEALS = Object.freeze({ burst: 8, perSecond: 0.2 });

export const keeperRoutes = {
  'POST /f/reveal': async (ctx, req) => {
    if (!ctx.keeper) throw new RouteError(503, 'no keeper is linked to this relay', 'KeeperUnavailable');
    const body = revealBody(await req.json());
    ctx.limiter.check(`f-reveal:${body.holding}`, PER_HOLDING_REVEALS, 'reveals for this holding');
    const r = await ctx.keeper.reveal(body);
    return { status: r.status, body: r.body };
  },

  'POST /f/nudge': async (ctx, req) => {
    if (!ctx.keeper) throw new RouteError(503, 'no keeper is linked to this relay', 'KeeperUnavailable');
    const r = await ctx.keeper.nudge(nudgeBody(await req.json()));
    return { status: r.status, body: r.body };
  },
};
