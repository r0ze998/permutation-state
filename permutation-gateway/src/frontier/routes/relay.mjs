// Sponsored player transactions (contract §8.3, §9.4):
//
//   GET  /f/relay [?citizen=]  {feePayer, blockhash, lastValidBlockHeight, programId, quota: {left, resetsAt}}
//   POST /f/relay {tx, requester?, requesterSig?}   one player shape (not Join) or one settle shape → {ok, signature}
//   POST /f/join  {tx, invite?}                     the wallet-signed Join → {ok, signature}
//
// Order of checks (each refusal costs nobody anything): exact parse → shape
// allowlist (sdk shapes.mjs: prefix, CU price 0, L(kind), accounts, signers;
// a Reveal is 400 UseRevealRoute) → the fee payer is the pool's and the
// season is this one → the authority's signature → replay and the signer's
// rate limit → the quota (checked, not spent) → co-sign → simulate with
// signatures, reading the fee payer's post-balance → the drain guard →
// charge the quota → send. A failed simulation charges nothing and sends
// nothing. The answer does not wait for confirmation: GET /f/tx/{signature}
// reports it.
import { createHash } from 'node:crypto';
import { PublicKey } from '@solana/web3.js';
import { encode as base58 } from '../../../client/src/base58.mjs';
import { fromBase64 } from '../../../client/src/bytes.mjs';
import { classify, JOIN_TAG } from '../../../client/src/frontier/shapes.mjs';
import { pubkeyBytes, wireTransaction } from '../../../client/src/solana-tx.mjs';
import { signTalk as signEd25519, verifyTalk as verifyEd25519 } from '../../../client/src/talk-node.mjs';
import { parseWire, signedBy } from '../../cosign.mjs';
import { RouteError } from '../../routes/errors.mjs';
import { simulateWatching } from '../chain.mjs';
import { dailyTxs, dayEnd, gameDay } from '../quota.mjs';
import { allowanceFor, checkRelayShape, drainGuard, feeOf, lamportsPerDay, quotaKeyOf } from '../shapes.mjs';

/** Per-signer limits (on top of the per-address ones and the quotas). */
export const FRONTIER_SIGNER_LIMITS = Object.freeze({ relay: { burst: 24, perSecond: 1 }, join: { burst: 4, perSecond: 0.05 } });

/** `tx` (parsed) with `keypairs` (web3.js) signing in their signer positions; the wire bytes. */
export function signAs(tx, keypairs) {
  const sigs = [...tx.signatures];
  for (const kp of keypairs) {
    const i = tx.signers.indexOf(kp.publicKey.toBase58());
    if (i < 0) throw new Error(`${kp.publicKey.toBase58()} is not a signer of this transaction`);
    sigs[i] = signEd25519(tx.message, kp);
  }
  return wireTransaction(tx.message, sigs);
}

/** A settle request's requester: its key if it signed the transaction's message with it, else null (anonymous). */
function requesterOf(tx, b) {
  if (b.requester === undefined && b.requesterSig === undefined) return null;
  let ok = false;
  try {
    const sig = fromBase64(String(b.requesterSig ?? ''));
    ok = sig.length === 64 && verifyEd25519(tx.message, sig, pubkeyBytes(b.requester));
  } catch { ok = false; }
  if (!ok) throw new RouteError(400, 'requesterSig must be the requester\'s ed25519 signature of the transaction message', 'BadSignature');
  return String(b.requester);
}

/** The game day and the lamport cap for quotas, from the chain. */
async function quotaFrame(ctx) {
  const season = await ctx.chain.season();
  const clock = await ctx.chain.clock();
  return { season, genesisTs: Number(season.GENESIS_TS), day: gameDay(clock.unixTimestamp, season.GENESIS_TS), lamportsCap: lamportsPerDay(season) };
}

/**
 * The common sponsoring path of POST /f/relay and POST /f/join, from the
 * classified `shape` on: returns the answer body. `extraSigners`: relay
 * keys that sign besides the fee payer (the join gate); `onSent`: after the
 * send (consumes an invite).
 */
async function sponsor(ctx, req, { tx, wire, shape, requester = null, extraSigners = [], limitKind = 'relay', lastValidBlockHeight, onSent }) {
  const { pool, connection, chain, quota, relayed, limiter, blockhashes } = ctx;
  checkRelayShape(shape, { pool, addresses: ctx.addresses });
  // Signature first: a forged transaction naming someone's key costs that key nothing.
  if (shape.authority && !signedBy(tx, shape.authority)) throw new RouteError(400, 'the authority\'s signature is missing or invalid', 'BadSignature');
  const replayKey = shape.authority ? `sig:${base58(tx.signatures[tx.signers.indexOf(shape.authority)])}` : `msg:${createHash('sha256').update(tx.message).digest('hex')}`;
  relayed.refuseRepeat(replayKey);
  const signerKey = shape.authority ?? requester;
  if (signerKey) limiter.check(`f-${limitKind}:${signerKey}`, FRONTIER_SIGNER_LIMITS[limitKind], 'relays for this key');
  const frame = await quotaFrame(ctx);
  const citizen = shape.name === 'FileTicket' ? await chain.citizen(shape.accounts.citizen) : null;
  const allowance = allowanceFor(shape, { season: frame.season, citizen });
  const key = quotaKeyOf(shape, { requester, ip: req.ip });
  quota.check(key, { day: frame.day, lamports: allowance, lamportsCap: frame.lamportsCap, genesisTs: frame.genesisTs });
  // Held while in flight (so concurrent requests cannot both pass the last
  // unit of a quota or send the same bytes twice), given back in full when
  // anything below refuses: a refused transaction is never charged.
  quota.charge(key, { day: frame.day, lamports: allowance });
  relayed.add(replayKey);
  let moved;
  let post;
  let signed;
  try {
    signed = signAs(tx, [pool.keypair(shape.feePayer), ...extraSigners]);
    // The fee payer's balance around the simulation: the larger reading, so a
    // top-up landing in between can never hide a drain (it can only refuse).
    const balance = async () => BigInt(await connection.getBalance(new PublicKey(shape.feePayer), 'confirmed'));
    const before = await balance();
    const sim = await simulateWatching(connection, signed, [shape.feePayer]);
    const after = await balance();
    post = sim.post[0];
    moved = drainGuard({ pre: before > after ? before : after, post, fee: feeOf(shape), allowance });
  } catch (e) {
    quota.refund(key, { day: frame.day, lamports: allowance });
    relayed.seen.delete(replayKey); // guards.mjs ReplayCache keeps its keys in `seen`
    throw e;
  }
  // Charged: the transaction and the lamports it really moves.
  quota.refund(key, { day: frame.day, lamports: allowance - moved, txs: 0 });
  if (post !== null) pool.note(shape.feePayer, post);
  const expiry = await blockhashes.expiryOf(tx.recentBlockhash, lastValidBlockHeight);
  const signature = await connection.sendRawTransaction(signed, { skipPreflight: true, maxRetries: 5 });
  ctx.sent.set(signature, { lastValidBlockHeight: expiry, kind: shape.name });
  onSent?.();
  ctx.log?.(`f/relay ${shape.name} ${signature} (fee payer ${shape.feePayer}, quota ${key}, moved ${moved})`);
  return { ok: true, signature };
}

function parseAndClassify(ctx, b) {
  const { wire, tx } = parseWire(b.tx);
  const shape = classify(tx, { programId: ctx.programId, wireBytes: wire.length });
  if (!shape.ok) throw new RouteError(400, `relay refused: ${shape.problem}`, shape.code);
  return { wire, tx, shape };
}

export const relayRoutes = {
  'GET /f/relay': async (ctx, req) => {
    const { blockhash, lastValidBlockHeight } = await ctx.blockhashes.latest();
    const citizen = req.url.searchParams.get('citizen');
    let quotaView = null;
    try {
      const frame = await quotaFrame(ctx);
      quotaView = citizen ? (({ left, resetsAt }) => ({ left, resetsAt }))(ctx.quota.status(`citizen:${citizen}`, frame))
        : { left: dailyTxs(frame.day), resetsAt: dayEnd(frame.day, frame.genesisTs) };
    } catch (e) {
      if (!(e instanceof RouteError)) throw e;
    }
    return { body: { feePayer: ctx.pool.draw().publicKey.toBase58(), blockhash, lastValidBlockHeight, programId: ctx.programId, quota: quotaView } };
  },

  'POST /f/relay': async (ctx, req) => {
    const b = await req.json();
    const { wire, tx, shape } = parseAndClassify(ctx, b);
    if (shape.tag === JOIN_TAG) throw new RouteError(400, 'relay refused: a Join goes through POST /f/join', 'RelayRejected');
    const requester = shape.kind === 'settle' ? requesterOf(tx, b) : null;
    return { body: await sponsor(ctx, req, { tx, wire, shape, requester, lastValidBlockHeight: b.lastValidBlockHeight }) };
  },

  'POST /f/join': async (ctx, req) => {
    const b = await req.json();
    const { wire, tx, shape } = parseAndClassify(ctx, b);
    if (shape.tag !== JOIN_TAG) throw new RouteError(400, 'relay refused: POST /f/join takes a Join only', 'RelayRejected');
    const season = await ctx.chain.season();
    const gate = base58(season.JOIN_GATE);
    const gated = season.JOIN_GATE.some(x => x !== 0);
    const named = shape.accounts.join_gate;
    if (!gated) {
      if (named !== undefined) throw new RouteError(400, 'relay refused: this season has no join gate; leave the gate account out', 'RelayRejected');
      return { body: await sponsor(ctx, req, { tx, wire, shape, limitKind: 'join', lastValidBlockHeight: b.lastValidBlockHeight }) };
    }
    if (named !== gate) throw new RouteError(400, 'relay refused: the Join must name the season\'s join gate as its last account (GET /f/season)', 'RelayRejected');
    if (!ctx.gateKey || ctx.gateKey.publicKey.toBase58() !== gate) throw new RouteError(503, 'this relay does not hold the season\'s join-gate key', 'GateUnavailable');
    // The wallet's signature before the invite is looked at: a forged Join spends no invite.
    if (!signedBy(tx, shape.authority)) throw new RouteError(400, 'the wallet\'s signature is missing or invalid', 'BadSignature');
    const nonce = ctx.invites?.reserve(b.invite);
    if (!nonce) throw new RouteError(403, 'this season needs a valid, unused invite to join', 'InviteRequired');
    try {
      return { body: await sponsor(ctx, req, { tx, wire, shape, extraSigners: [ctx.gateKey], limitKind: 'join', lastValidBlockHeight: b.lastValidBlockHeight,
        onSent: () => ctx.invites.consume(nonce) }) };
    } finally {
      ctx.invites.release(nonce);
    }
  },
};
