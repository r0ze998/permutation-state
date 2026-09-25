// GameClient: everything an outside member (an AI agent, a bot, a tool)
// needs, over plain HTTP (Game Design V5).
//
//   game server (permutation-server `play`)   what your nation knows: its fogged
//                                             view, government, previews, dry runs
//   gateway     (permutation-gateway)          the chain: x402 registration, relay
//                                             of transactions you signed yourself
//
// You are a member of one nation. If you hold an office (general, steward,
// science, diplomat), you order within it; every member proposes, supports
// proposals, votes and recalls. Nothing here holds authority: your session
// key signs, the program on the MagicBlock ER checks that signature, and the
// gateway only pays the fee. As an officer your reasoning is committed as a
// digest with each office's batch and revealed in a later one
// (decision.mjs), so anyone can check it afterwards (V5 D17).
//
// This module is the facade; the parts live in http.mjs (transport and
// errors), offices.mjs (which office gives which order), batch.mjs (packing
// a batch), x402-client.mjs (registration) and keys.mjs (key files).
import { PublicKey, Transaction } from '@solana/web3.js';
import { packBatch } from './batch.mjs';
import { ChainClient } from './chain.mjs';
import { BATCH_BYTES } from './codec.mjs';
import { commit } from './decision.mjs';
import { DEFAULT_GATEWAY, DEFAULT_SERVER, errorCode, GameError, HttpError, requestJson } from './http.mjs';
import { splitByOffice } from './offices.mjs';
import { joinViaX402 } from './x402-client.mjs';

// Re-exported for existing importers of game.mjs.
export { BATCH_BYTES, GameError, HttpError };
export { officeOf } from './offices.mjs';
export { loadOrCreateKeypair } from './keys.mjs';

export class GameClient {
  /**
   * @param {object} o
   * @param {string} o.server   game server, e.g. http://127.0.0.1:4185
   * @param {string} o.gateway  chain gateway, e.g. http://127.0.0.1:4191
   * @param {number} [o.member] your member id, once you have one
   * @param {Keypair} [o.session] your session key (signs your batches and governance actions)
   */
  constructor({ server = DEFAULT_SERVER, gateway = DEFAULT_GATEWAY, member = null, civ = null, session = null } = {}) {
    this.server = server.replace(/\/$/, '');
    this.gateway = gateway.replace(/\/$/, '');
    this.member = member;
    this.civ = civ;
    this.session = session;
    this.decisions = new Map(); // "tick:role" -> commitment not yet known to be revealed
    this.chain = null;
  }

  /** GET / POST JSON; failures throw `HttpError` (`.status`, `.code`). */
  get(base, path) { return requestJson(`${base}${path}`); }
  post(base, path, body) { return requestJson(`${base}${path}`, { method: 'POST', body }); }
  viewer() { return this.member !== null ? { member: String(this.member) } : this.civ !== null ? { civ: String(this.civ) } : {}; }

  // ------------------------------------------------------------ reading
  /** Nations, member counts and projections; the season's phase. */
  lobby() { return this.get(this.server, '/api/lobby'); }
  /** Static map: tiles `[q, r, terrain, river, resource]`, trade hubs. */
  map() { return this.get(this.server, '/api/map'); }
  /**
   * Your nation's fogged view: world, government (`gov`: offices, candidates,
   * recalls, proposals), achievements, the payout projection, and
   * `decision.obsRoot`, which an officer's commitment binds to.
   */
  state() { return this.get(this.server, `/api/state?${new URLSearchParams(this.viewer())}`); }
  /** kind: unit | city | research | diplomacy | path | amm. */
  preview(kind, params = {}) {
    const q = new URLSearchParams({ ...this.viewer(), ...Object.fromEntries(Object.entries(params).map(([k, v]) => [k, String(v)])) });
    return this.get(this.server, `/api/preview/${kind}?${q}`);
  }
  /**
   * Dry run per office: `{ok, offices: [{role, cost, spendable, error, holder}], warnings[]}`.
   * `warnings` are orders that would be skipped when the tick resolves, judged from what your nation sees.
   */
  validate(orders) { return this.post(this.server, `/api/validate?${new URLSearchParams(this.viewer())}`, { civ: this.civ, orders }); }
  /** Committed and revealed decisions of past ticks. */
  decisionLog(limit = 120) { return this.get(this.server, `/api/decisions?limit=${limit}`); }
  /** Season account (members, pool, payouts) and the gateway's phase. */
  season() { return this.get(this.gateway, '/season'); }
  /** Offices you hold in `view` (role names). */
  myOffices(view) { return (view.gov?.offices ?? []).filter(o => o.holder?.id === this.member).map(o => o.role); }

  /** Resolve after the world moves past `tick` (polls the game server). */
  async waitForTick(tick, { pollMs = 700, timeoutMs = 10 * 60_000 } = {}) {
    const until = Date.now() + timeoutMs;
    for (;;) {
      const s = await this.state().catch(() => null);
      if (s && (s.tick > tick || s.over)) return s;
      if (Date.now() > until) throw new GameError('TickTimeout', `tick ${tick} did not resolve within ${timeoutMs} ms`);
      await new Promise(r => setTimeout(r, pollMs));
    }
  }

  // ------------------------------------------------------------ registration (x402)
  /** Localnet and devnet: a token account with 100 of the gateway's test USDC (no value) for `owner`. */
  faucet(owner) { return this.post(this.gateway, '/faucet', { owner: owner.toBase58() }); }

  /**
   * Pay the entry fee over HTTP 402 and become a member of nation `civ`
   * (see x402-client.mjs). `stand` (office names) and `votes` (member ids
   * per office) are your candidacy and your votes in the first election.
   * @returns {{member, civ, nation, signature, requirements, paymentResponse}}
   */
  async joinViaX402(o) {
    const joined = await joinViaX402(this.gateway, o);
    this.member = joined.member;
    this.civ = joined.civ;
    this.session = o.session;
    return joined;
  }

  // ------------------------------------------------------------ playing
  async chainClient() {
    if (!this.chain) {
      const s = await this.season();
      this.chain = new ChainClient(s.programId, BigInt(s.season.seasonId));
    }
    return this.chain;
  }

  /** Sign `ixs` with the session key and relay them through the gateway (which pays the fee, on the ER). */
  async relay(ixs) {
    const relay = await this.get(this.gateway, '/relay');
    const tx = new Transaction().add(...ixs);
    tx.feePayer = new PublicKey(relay.feePayer);
    tx.recentBlockhash = relay.blockhash;
    tx.partialSign(this.session);
    return this.post(this.gateway, '/relay', { tx: tx.serialize({ requireAllSignatures: false }).toString('base64'), lastValidBlockHeight: relay.lastValidBlockHeight });
  }

  /** Commitments of `role` from before `tick` whose reveal has not gone out in a batch that resolved. */
  pendingReveals(role, tick) {
    return [...this.decisions.values()].filter(d => d.role === role && d.tick < tick && !(d.sentIn !== undefined && d.sentIn < tick));
  }

  /**
   * Submit this tick's orders for the offices you hold: one batch per office
   * (an empty one ends an idle office's turn), each committing to its own
   * decision digest and revealing that office's earlier decisions (at most
   * 3, as many as fit). Orders for offices you do not hold are not sent;
   * they come back in `notHeld` (propose them instead).
   *
   * Every batch is packed before any is sent: if one does not fit a
   * transaction, nothing is sent (`{ok: false, code: 'BatchTooLarge'}`).
   * Each office is relayed on its own; `offices` lists each one's
   * signature or `error`/`code`, and `ok` is true only if all went through
   * (`code` is then the first failure's, e.g. `TickFrozen`).
   * @param {object} o
   * @param {object[]} o.orders   order DTOs (see llms.txt)
   * @param {string} o.policy      short name of how you decide, e.g. "rule-agent/v2"
   * @param {string} [o.rationale] why (≤512 bytes); revealed after the tick resolves
   * @param {object} [o.adopt]     proposal ids to adopt, per office: {Science: [3]}
   * @param {object} [o.view]      the state you decided from (saves a request)
   */
  async submit({ orders, policy, rationale = '', adopt = {}, view = null }) {
    if (this.member === null || !this.session) throw new GameError('NotAMember', 'not a member: join first (joinViaX402) or pass member and session');
    const v = view ?? await this.state();
    const tick = v.tick;
    if (!v.decision?.obsRoot) throw new GameError('NoObservation', 'the game server has no observation root for this tick yet');
    const held = this.myOffices(v);
    const { byOffice, notHeld } = splitByOffice(orders, held, v);
    const plans = held.map(role => ({
      role,
      own: byOffice[role],
      commitment: { ...commit({ tick, obsRoot: v.decision.obsRoot, policy, text: rationale }), role },
      ...packBatch({ orders: byOffice[role], pending: this.pendingReveals(role, tick) }),
    }));
    const tooLarge = plans.filter(p => !p.fits);
    if (tooLarge.length) {
      const error = tooLarge.map(p => `${p.role}: batch too large for one transaction (${p.used} > ${BATCH_BYTES} bytes of orders)`).join('; ');
      return { ok: false, tick, code: 'BatchTooLarge', error, offices: [], notHeld, warnings: [] };
    }
    const check = await this.validate(orders.filter(o => !notHeld.includes(o)));
    const chain = await this.chainClient();
    const offices = [];
    for (const p of plans) {
      try {
        const r = await this.relay(chain.submitOrders({ signer: this.session.publicKey, civ: this.civ, role: p.role, tick,
          decisionDigest: Buffer.from(p.commitment.digest, 'hex'), orders: p.orders, adopt: adopt[p.role] ?? [] }));
        this.decisions.set(`${tick}:${p.role}`, p.commitment);
        for (const d of p.reveals) d.sentIn = tick;
        offices.push({ role: p.role, orders: p.own.length, digest: p.commitment.digest, revealed: p.reveals.map(d => d.tick), signature: r.signature });
      } catch (e) {
        offices.push({ role: p.role, orders: p.own.length, error: e.message, code: errorCode(e), status: e.status });
      }
    }
    // Forget reveals that went out in a batch that has since resolved.
    for (const [k, d] of this.decisions) if (d.sentIn !== undefined && d.sentIn < tick) this.decisions.delete(k);
    const failed = offices.filter(o => o.error);
    const failure = failed.length ? { error: failed.map(o => `${o.role}: ${o.error}`).join('; '), code: failed[0].code, status: failed[0].status } : {};
    return { ok: !failed.length, tick, offices, notHeld, warnings: check.warnings ?? [], validation: check.offices, ...failure };
  }

  /**
   * After the season is finalized: claim your prize and your share of the
   * nation treasury into `usdcAccount` (a test-USDC account your wallet
   * owns, e.g. the one from `faucet`). Your wallet signs; the gateway only
   * pays the fee.
   */
  async claim({ wallet, usdcAccount }) {
    const relay = await this.get(this.gateway, '/claim-relay');
    if (relay.status !== 'Finalized') throw new GameError('NotFinalized', `the season is ${relay.status}; claims open once it is Finalized`);
    const chain = await this.chainClient();
    const tx = new Transaction().add(...chain.claim({ wallet: wallet.publicKey, dest: new PublicKey(usdcAccount), mint: new PublicKey(relay.mint) }));
    tx.feePayer = new PublicKey(relay.feePayer);
    tx.recentBlockhash = relay.blockhash;
    tx.partialSign(wallet);
    return this.post(this.gateway, '/claim-relay', { tx: tx.serialize({ requireAllSignatures: false }).toString('base64'), lastValidBlockHeight: relay.lastValidBlockHeight });
  }

  /** Sign and relay one governance action (V5 §5): Stand, Vote, Propose, Support, Recall. */
  async gov(action) {
    if (this.member === null || !this.session) throw new GameError('NotAMember', 'not a member: join first');
    const chain = await this.chainClient();
    const r = await this.relay(chain.submitGov({ signer: this.session.publicKey, civ: this.civ, member: this.member, action }));
    return { ok: true, signature: r.signature };
  }
  propose(role, orders) { return this.gov({ type: 'Propose', role, orders }); }
  support(proposal) { return this.gov({ type: 'Support', proposal }); }
  vote(role, candidate) { return this.gov({ type: 'Vote', role, candidate }); }
  stand(roles) { return this.gov({ type: 'Stand', roles }); }
  recall(role) { return this.gov({ type: 'Recall', role }); }
}
