// Operator AI members (V5 §18.2) and members' messages (§18.7).
//
//   GET  /roster            public: how many operator AI members, the bounty, and the ones
//                           revealed so far (a home city conquered, or the season over): member,
//                           nation and salt, which anyone checks against the member's tag
//   GET  /operator/roster   operator only (Authorization: Bearer <operator token>, operator
//                           listener): every member this gateway registered, `hosted` 'ai' (the
//                           operator's AI members, run by the game server) or 'external', and the
//                           AI members' salts in roster order
//   POST /roster/announce   operator only: {member} an AI member whose home city was conquered;
//                           its salt becomes public
//   POST /talk              {member, to?, text, tick, signature}: a message, signed over `talkBytes`
//                           by the key the member was seated with in the world (its session key;
//                           a later member that registered someone's session key again is seated
//                           with a key nobody can sign with, seats.mjs); on the operator listener
//                           the operator may omit the signature for its AI members (the gateway
//                           signs). On the public listener an unsigned message is 400 SignatureRequired
//   GET  /talk?since=N      every message from id N, public, with the tick's anchor once sent
import { createHash, timingSafeEqual } from 'node:crypto';
import { fromHex } from '../../client/src/bytes.mjs';
import { NATIONS } from '../../client/src/codec.mjs';
import { RouteError } from './errors.mjs';
import { aiMembers } from '../season.mjs';
import { seatedKeyMap } from '../seats.mjs';
import { signTalk, TalkBook, talkBytes } from '../talk.mjs';

const digest = s => createHash('sha256').update(String(s)).digest();

/**
 * Throws 403 unless the request came to the operator listener with the
 * operator token (compared in constant time).
 */
export function requireOperator({ cfg }, req) {
  const onOperatorListener = (req.surface ?? 'operator') === 'operator';
  const auth = req.headers?.authorization ?? '';
  const ok = !!cfg.operatorToken && timingSafeEqual(digest(auth), digest(`Bearer ${cfg.operatorToken}`));
  if (!onOperatorListener || !ok) throw new RouteError(403, 'operator only', 'OperatorOnly');
}

const publicAi = m => ({ member: m.index, civ: m.civ, salt: m.salt });

export const rosterRoutes = {
  'GET /roster': async ({ store }) => {
    const state = store.state;
    const r = state.roster ?? { aiCount: 0 };
    const announced = new Set(r.announced ?? []);
    const shown = aiMembers(state).filter(m => r.revealed || announced.has(m.index));
    return { body: { aiCount: r.aiCount ?? 0, bountyEach: r.bountyEach ?? '0', bond: r.bond ?? '0', revealed: !!r.revealed,
      ai: shown.map(publicAi) } };
  },

  'GET /operator/roster': async (ctx, req) => {
    requireOperator(ctx, req);
    const state = ctx.store.state;
    return { body: { members: state.members.map(m => ({ member: m.index, civ: m.civ, hosted: m.hosted })),
      ai: aiMembers(state).map(publicAi), bountyEach: state.roster?.bountyEach ?? '0' } };
  },

  'POST /roster/announce': async (ctx, req) => {
    requireOperator(ctx, req);
    const b = await req.json();
    const state = ctx.store.state;
    const m = aiMembers(state).find(x => x.index === b.member);
    if (!m) throw new RouteError(404, 'not an operator AI member', 'NotOnRoster');
    state.roster ??= {};
    state.roster.announced = [...new Set([...(state.roster.announced ?? []), m.index])];
    ctx.store.save();
    ctx.log(`bounty: AI member ${m.index} (${m.name}) lived in a conquered city; its salt is public`);
    return { body: { ok: true, ...publicAi(m) } };
  },

  'POST /talk': async (ctx, req) => {
    const b = await req.json();
    const { store, crank, registry } = ctx;
    const members = await registry.list();
    const member = members.find(m => m.index === b.member);
    if (!member) throw new RouteError(404, 'no such member', 'NoSuchMember');
    const open = crank.snapshot?.nations?.[0]?.openTick ?? 0;
    const season = BigInt(store.state.seasonId);
    // The key the member acts with in the world: a copied session key speaks for its first holder only.
    const seatedKey = seatedKeyMap({ members, seasonId: season, seating: store.state.seating ?? [] }).get(b.member);
    const to = b.to == null ? null : b.to.civ !== undefined ? { civ: b.to.civ } : { member: b.to.member };
    let signature = b.signature ? fromHex(b.signature) : null;
    // A signed message names its tick: the open one, or the one before (it
    // may have resolved while the message travelled).
    const tick = signature && Number.isInteger(b.tick) ? b.tick : open;
    if (tick > open || tick + 1 < open) throw new RouteError(400, `tick ${tick} is not the open tick ${open}`, 'TalkRefused');
    // A tick's messages are anchored once (one PS_TALK root per tick): late
    // ones for an anchored tick (the one before, or the snapshot's open tick
    // right after it resolved) are refused, not anchored a second time.
    const book = new TalkBook(store.state);
    if (book.isAnchored(tick)) throw new RouteError(400, `tick ${tick} is already anchored`, 'TalkRefused');
    let text = b.text;
    if (!signature) {
      // An AI member's message, sent by the game server that runs it (its
      // answer may be phrased by the operator's language model).
      if ((req.surface ?? 'operator') !== 'operator') throw new RouteError(400, 'signature required', 'SignatureRequired');
      requireOperator(ctx, req);
      const key = ctx.hostedKey(b.member);
      if (!key) throw new RouteError(400, 'signature required', 'SignatureRequired');
      if (b.draft && ctx.advisor) text = await ctx.advisor.phrase({ fallback: text, draft: b.draft, nation: NATIONS[member.civ] });
      signature = signTalk(talkBytes({ season, tick, member: b.member, to, text: text ?? '' }), key);
    }
    let m;
    try {
      m = book.add({ season, tick, member: b.member, to, text, signature, publicKey: seatedKey });
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
