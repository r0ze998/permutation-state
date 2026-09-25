// Operator AI members (V5 §18.2) and members' messages (§18.7).
//
//   GET  /roster            public: how many operator AI members, the bounty, and the ones
//                           revealed so far (a home city conquered, or the season over): member,
//                           nation and salt, which anyone checks against the member's tag
//   GET  /operator/roster   operator only (Authorization: Bearer <operator token>): which
//                           members this gateway hosts and the AI members' salts, for the game
//                           server that runs them
//   POST /roster/announce   operator only: {member} an AI member whose home city was conquered;
//                           its salt becomes public
//   POST /talk              {member, to?, text, signature?}: a message, signed by the member's
//                           session key over `talkBytes` (the operator may omit the signature for
//                           a member this gateway hosts: the gateway signs)
//   GET  /talk?since=N      every message from id N, public, with the tick's anchor once sent
import { PublicKey } from '@solana/web3.js';
import { NATIONS } from '../../client/src/codec.mjs';
import { RouteError } from './errors.mjs';
import { aiMembers } from '../season.mjs';
import { signTalk, TalkBook, talkBytes } from '../talk.mjs';

/** Throws 403 unless the request carries the operator token. */
export function requireOperator({ cfg }, req) {
  const auth = req.headers?.authorization ?? '';
  if (!cfg.operatorToken || auth !== `Bearer ${cfg.operatorToken}`) throw new RouteError(403, 'operator only', 'OperatorOnly');
}

const publicAi = (state, m) => ({ member: m.index, civ: m.civ, salt: m.salt });

export const rosterRoutes = {
  'GET /roster': async ({ store }) => {
    const state = store.state;
    const r = state.roster ?? { aiCount: 0 };
    const announced = new Set(r.announced ?? []);
    const shown = aiMembers(state).filter(m => r.revealed || announced.has(m.index));
    return { body: { aiCount: r.aiCount ?? 0, bountyEach: r.bountyEach ?? '0', bond: r.bond ?? '0', revealed: !!r.revealed,
      ai: shown.map(m => publicAi(state, m)) } };
  },

  'GET /operator/roster': async (ctx, req) => {
    requireOperator(ctx, req);
    const state = ctx.store.state;
    return { body: { members: state.members.map(m => ({ member: m.index, civ: m.civ, hosted: m.hosted })),
      ai: aiMembers(state).map(m => publicAi(state, m)), bountyEach: state.roster?.bountyEach ?? '0' } };
  },

  'POST /roster/announce': async (ctx, req) => {
    requireOperator(ctx, req);
    const b = await req.json();
    const state = ctx.store.state;
    const m = aiMembers(state).find(x => x.index === b.member);
    if (!m) throw new RouteError(404, 'not an operator AI member', 'NotOnRoster');
    state.roster.announced = [...new Set([...(state.roster.announced ?? []), m.index])];
    ctx.store.save();
    ctx.log(`bounty: AI member ${m.index} (${m.name}) lived in a conquered city; its salt is public`);
    return { body: { ok: true, ...publicAi(state, m) } };
  },

  'POST /talk': async (ctx, req) => {
    const b = await req.json();
    const { store, crank, registry } = ctx;
    const member = (await registry.list()).find(m => m.index === b.member);
    if (!member) throw new RouteError(404, 'no such member', 'NoSuchMember');
    const open = crank.snapshot?.nations?.[0]?.openTick ?? 0;
    const season = BigInt(store.state.seasonId);
    const to = b.to == null ? null : b.to.civ !== undefined ? { civ: b.to.civ } : { member: b.to.member };
    let signature = b.signature ? Buffer.from(b.signature, 'hex') : null;
    // A signed message names its tick: the open one, or the one before (it
    // may have resolved while the message travelled).
    const tick = signature && Number.isInteger(b.tick) ? b.tick : open;
    if (tick > open || tick + 1 < open) throw new RouteError(400, `tick ${tick} is not the open tick ${open}`, 'TalkRefused');
    let text = b.text;
    if (!signature) {
      // A hosted member's message, sent by the game server that runs it; an
      // AI member's answer may be phrased by the operator's language model.
      requireOperator(ctx, req);
      const key = ctx.hostedKey(b.member);
      if (!key) throw new RouteError(400, 'signature required', 'SignatureRequired');
      if (b.draft && ctx.advisor) text = await ctx.advisor.phrase({ fallback: text, draft: b.draft, nation: NATIONS[member.civ] });
      signature = signTalk(talkBytes({ season, tick, member: b.member, to, text: text ?? '' }), key);
    }
    const book = new TalkBook(store.state);
    let m;
    try {
      m = book.add({ season, tick, member: b.member, to, text, signature, publicKey: new PublicKey(member.session).toBytes() });
    } catch (e) {
      throw new RouteError(400, e.message, 'TalkRefused');
    }
    store.save();
    return { body: { ok: true, id: m.id, tick } };
  },

  'GET /talk': async ({ store }, req) => {
    const since = Number(req.url.searchParams.get('since') || 0);
    return { body: { messages: new TalkBook(store.state).since(since) } };
  },
};
