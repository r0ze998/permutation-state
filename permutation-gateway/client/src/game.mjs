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
import { Keypair, PublicKey, Transaction } from '@solana/web3.js';
import { ChainClient } from './chain.mjs';
import { commit, revealOrder } from './decision.mjs';
import { Writer } from './borsh.mjs';
import { encodeOrder, NOBODY, ROLES, roleMask } from './codec.mjs';

/** Encoded orders that fit in one SubmitOrders transaction (packet limit 1232 bytes); matches the server's BATCH_BYTES. */
export const BATCH_BYTES = 820;
const encodedLen = o => encodeOrder(new Writer(), o).toBytes().length;

const OFFICE = {
  Attack: 'General', Raze: 'General',
  FoundCity: 'Steward', SetQueue: 'Steward', SetFocus: 'Steward', Purchase: 'Steward',
  SetResearch: 'Science',
  DeclareWar: 'Diplomat', ProposePeace: 'Diplomat', AcceptPeace: 'Diplomat', ProposeNap: 'Diplomat', AcceptNap: 'Diplomat', BreakNap: 'Diplomat',
  ProposeAlliance: 'Diplomat', AcceptAlliance: 'Diplomat', LeaveAlliance: 'Diplomat', SendEnvoy: 'Diplomat', Transfer: 'Diplomat',
  MarketTrade: 'Diplomat', ExchangeOrder: 'Diplomat',
  ConsentWar: 'General', ConsentSpend: 'General',
};

/**
 * The office an order belongs to (V5 §5.1). Unit orders depend on the unit:
 * settlers are the steward's, armies and scouts the general's.
 * `ConsentWar`/`ConsentSpend` are listed under the general (the steward may
 * give them too).
 */
export function officeOf(order, view) {
  if (order.type === 'MoveUnit' || (order.type === 'SetStanding' && order.target.kind === 'Unit')) {
    const id = order.type === 'MoveUnit' ? order.unit : order.target.id;
    return view?.units?.find(u => u.id === id)?.type === 'Settler' ? 'Steward' : 'General';
  }
  if (order.type === 'SetStanding') return 'Steward';
  return OFFICE[order.type] ?? null;
}

export class HttpError extends Error {
  constructor(status, body, url) {
    super(`${url}: HTTP ${status} ${body?.error ?? ''}`.trim());
    this.status = status; this.body = body; this.url = url;
  }
}

async function http(url, { method = 'GET', body, headers = {} } = {}) {
  const res = await fetch(url, { method, headers: { ...(body ? { 'Content-Type': 'application/json' } : {}), ...headers }, body: body ? JSON.stringify(body) : undefined });
  const text = await res.text();
  let json;
  try { json = text ? JSON.parse(text) : null; } catch { json = { error: text }; }
  return { status: res.status, headers: res.headers, body: json };
}

export class GameClient {
  /**
   * @param {object} o
   * @param {string} o.server   game server, e.g. http://127.0.0.1:4185
   * @param {string} o.gateway  chain gateway, e.g. http://127.0.0.1:4191
   * @param {number} [o.member] your member id, once you have one
   * @param {Keypair} [o.session] your session key (signs your batches and governance actions)
   */
  constructor({ server = 'http://127.0.0.1:4185', gateway = 'http://127.0.0.1:4191', member = null, civ = null, session = null } = {}) {
    this.server = server.replace(/\/$/, '');
    this.gateway = gateway.replace(/\/$/, '');
    this.member = member;
    this.civ = civ;
    this.session = session;
    this.decisions = new Map(); // "tick:role" -> commitment not yet known to be revealed
    this.chain = null;
  }

  async get(base, path) {
    const r = await http(`${base}${path}`);
    if (r.status >= 400) throw new HttpError(r.status, r.body, `${base}${path}`);
    return r.body;
  }
  async post(base, path, body) {
    const r = await http(`${base}${path}`, { method: 'POST', body });
    if (r.status >= 400) throw new HttpError(r.status, r.body, `${base}${path}`);
    return r.body;
  }
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
      if (Date.now() > until) throw new Error(`tick ${tick} did not resolve within ${timeoutMs} ms`);
      await new Promise(r => setTimeout(r, pollMs));
    }
  }

  // ------------------------------------------------------------ registration (x402)
  /** Localnet and devnet: a token account with 100 of the gateway's test USDC (no value) for `owner`. */
  faucet(owner) { return this.post(this.gateway, '/faucet', { owner: owner.toBase58() }); }

  /**
   * Pay the entry fee over HTTP 402 and become a member of nation `civ`.
   * 1. POST /x402/join → 402 with PaymentRequirements (scheme "exact").
   * 2. Sign the program's Register (it moves exactly the entry fee, plus any
   *    treasury `deposit`, from `usdcAccount` into the season vault) with
   *    `wallet`; the facilitator is the fee payer and adds its signature.
   * 3. POST again with X-PAYMENT → 200 + X-PAYMENT-RESPONSE.
   * `stand` (office names) and `votes` (member ids per office) are your
   * candidacy and your votes in the first election.
   * @returns {{member, civ, nation, signature, requirements, paymentResponse}}
   */
  async joinViaX402({ wallet, session, civ, name, kind = 1, usdcAccount, stand = [], votes = [], deposit = 0n, attestation }) {
    const url = `${this.gateway}/x402/join`;
    const first = await http(url, { method: 'POST', body: { civ, name } });
    if (first.status !== 402) throw new HttpError(first.status, first.body, url);
    const req = first.body.accepts?.[0];
    if (!req || req.scheme !== 'exact') throw new Error('gateway did not offer the "exact" scheme');
    const x = req.extra;
    const chain = new ChainClient(x.programId, BigInt(x.seasonId));
    // Never sign for a different season or vault than the requirements name.
    if (chain.season.toBase58() !== x.accounts.season || chain.vault.toBase58() !== req.payTo) throw new Error('payment requirements do not match the season PDAs');
    const pick = civ ?? x.nations.reduce((a, b) => (b.members < a.members ? b : a)).civ; // default: the smallest nation
    const tx = new Transaction();
    tx.add(...chain.register({ wallet: wallet.publicKey, feePayer: new PublicKey(x.feePayer), civ: pick, walletToken: new PublicKey(usdcAccount),
      mint: new PublicKey(req.asset), name, kind, session: session.publicKey, attestation, stand: roleMask(stand),
      votes: ROLES.map((_, i) => votes[i] ?? NOBODY), deposit: BigInt(deposit) }));
    tx.feePayer = new PublicKey(x.feePayer);
    tx.recentBlockhash = x.recentBlockhash;
    tx.partialSign(wallet);
    const payment = { x402Version: 1, scheme: 'exact', network: req.network, payload: { transaction: tx.serialize({ requireAllSignatures: false }).toString('base64') } };
    const second = await http(url, { method: 'POST', body: { civ: pick, name }, headers: { 'X-PAYMENT': Buffer.from(JSON.stringify(payment)).toString('base64') } });
    if (second.status !== 200) throw new HttpError(second.status, second.body, url);
    const header = second.headers.get('x-payment-response');
    this.member = second.body.member;
    this.civ = second.body.civ;
    this.session = session;
    return { ...second.body, requirements: req, paymentResponse: header ? JSON.parse(Buffer.from(header, 'base64').toString('utf8')) : null };
  }

  // ------------------------------------------------------------ playing
  async chainClient() {
    if (!this.chain) {
      const s = await this.season();
      this.chain = new ChainClient(s.programId, BigInt(s.season.seasonId));
    }
    return this.chain;
  }

  async relay(ixs) {
    const relay = await this.get(this.gateway, '/relay');
    const tx = new Transaction().add(...ixs);
    tx.feePayer = new PublicKey(relay.feePayer);
    tx.recentBlockhash = relay.blockhash;
    tx.partialSign(this.session);
    return this.post(this.gateway, '/relay', { tx: tx.serialize({ requireAllSignatures: false }).toString('base64') });
  }

  /**
   * Submit this tick's orders for the offices you hold: one batch per office
   * (an empty one ends an idle office's turn),
   * each committing to its own decision digest and revealing that office's
   * earlier decisions (at most 3, as many as fit). Orders for offices you do
   * not hold are not sent; they come back in `notHeld` (propose them instead).
   * @param {object} o
   * @param {object[]} o.orders   order DTOs (see llms.txt)
   * @param {string} o.policy      short name of how you decide, e.g. "rule-agent/v2"
   * @param {string} [o.rationale] why (≤512 bytes); revealed after the tick resolves
   * @param {object} [o.adopt]     proposal ids to adopt, per office: {Science: [3]}
   * @param {object} [o.view]      the state you decided from (saves a request)
   */
  async submit({ orders, policy, rationale = '', adopt = {}, view = null }) {
    if (this.member === null || !this.session) throw new Error('not a member: join first (joinViaX402) or pass member and session');
    const v = view ?? await this.state();
    const tick = v.tick;
    if (!v.decision?.obsRoot) throw new Error('the game server has no observation root for this tick yet');
    const held = this.myOffices(v);
    const byOffice = Object.fromEntries(ROLES.map(r => [r, []]));
    const notHeld = [];
    for (const o of orders) {
      const r = officeOf(o, v);
      if (held.includes(r)) byOffice[r].push(o);
      else if ((o.type === 'ConsentWar' || o.type === 'ConsentSpend') && held.some(h => h !== 'Diplomat')) byOffice[held.find(h => h !== 'Diplomat')].push(o);
      else notHeld.push(o);
    }
    const check = await this.validate(orders.filter(o => !notHeld.includes(o)));
    const chain = await this.chainClient();
    const sent = [];
    for (const role of held) {
      const own = byOffice[role];
      const pending = [...this.decisions.values()].filter(d => d.role === role && d.tick < tick && !(d.sentIn !== undefined && d.sentIn < tick));
      // An office with nothing to do still seals an (empty) batch: it ends
      // that office's turn, so the tick can resolve as soon as every office is in.
      const c = { ...commit({ tick, obsRoot: v.decision.obsRoot, policy, text: rationale }), role };
      const reveals = [];
      let used = own.reduce((n, o) => n + encodedLen(o), 0);
      for (const d of pending.sort((a, b) => a.tick - b.tick).slice(0, 3)) {
        const n = encodedLen(revealOrder(d));
        if (used + n > BATCH_BYTES) break;
        used += n;
        reveals.push(d);
      }
      if (used > BATCH_BYTES) return { ok: false, tick, error: `${role}: batch too large for one transaction (${used} > ${BATCH_BYTES} bytes of orders)` };
      const all = [...own, ...reveals.map(revealOrder)];
      const r = await this.relay(chain.submitOrders({ signer: this.session.publicKey, civ: this.civ, role, tick, decisionDigest: Buffer.from(c.digest, 'hex'), orders: all, adopt: adopt[role] ?? [] }));
      this.decisions.set(`${tick}:${role}`, c);
      for (const d of reveals) d.sentIn = tick;
      sent.push({ role, orders: own.length, digest: c.digest, revealed: reveals.map(d => d.tick), signature: r.signature });
    }
    // Forget reveals that went out in a batch that has since resolved.
    for (const [k, d] of this.decisions) if (d.sentIn !== undefined && d.sentIn < tick) this.decisions.delete(k);
    return { ok: true, tick, offices: sent, notHeld, warnings: check.warnings ?? [], validation: check.offices };
  }

  /**
   * After the season is finalized: claim your prize and your share of the
   * nation treasury into `usdcAccount` (a test-USDC account your wallet
   * owns, e.g. the one from `faucet`). Your wallet signs; the gateway only
   * pays the fee.
   */
  async claim({ wallet, usdcAccount }) {
    const relay = await this.get(this.gateway, '/claim-relay');
    if (relay.status !== 'Finalized') throw new Error(`the season is ${relay.status}; claims open once it is Finalized`);
    const chain = await this.chainClient();
    const tx = new Transaction().add(...chain.claim({ wallet: wallet.publicKey, dest: new PublicKey(usdcAccount), mint: new PublicKey(relay.mint) }));
    tx.feePayer = new PublicKey(relay.feePayer);
    tx.recentBlockhash = relay.blockhash;
    tx.partialSign(wallet);
    return this.post(this.gateway, '/claim-relay', { tx: tx.serialize({ requireAllSignatures: false }).toString('base64') });
  }

  /** Sign and relay one governance action (V5 §5): Stand, Vote, Propose, Support, Recall. */
  async gov(action) {
    if (this.member === null || !this.session) throw new Error('not a member: join first');
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

/** A keypair persisted as a Solana CLI style JSON array (created on first use). */
export async function loadOrCreateKeypair(file) {
  const fs = await import('node:fs');
  const path = await import('node:path');
  if (fs.existsSync(file)) return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(file, 'utf8'))));
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const kp = Keypair.generate();
  fs.writeFileSync(file, JSON.stringify(Array.from(kp.secretKey)), { mode: 0o600 });
  return kp;
}
