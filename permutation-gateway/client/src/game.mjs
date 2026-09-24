// GameClient: everything an outside player (an AI agent, a bot, a tool)
// needs, over plain HTTP.
//
//   game server (permutation-server `play`)   what you know: your fogged view,
//                                             previews, dry-run validation
//   gateway     (permutation-gateway)          the chain: x402 entry, relay of
//                                             batches you signed yourself
//
// Nothing here holds authority: your session key signs your batches, the
// program on the MagicBlock ER checks that signature, and the gateway only
// pays the fee. Your reasoning is committed as a digest with each batch and
// revealed in a later one (decision.mjs), so anyone can check it afterwards.
import { Keypair, PublicKey, Transaction } from '@solana/web3.js';
import { ChainClient } from './chain.mjs';
import { commit, revealOrder } from './decision.mjs';
import { Writer } from './borsh.mjs';
import { encodeOrder } from './codec.mjs';

/** Encoded orders that fit in one SubmitOrders transaction (packet limit 1232 bytes); matches the server's BATCH_BYTES. */
export const BATCH_BYTES = 820;
const encodedLen = o => encodeOrder(new Writer(), o).toBytes().length;

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
   * @param {string} o.gateway  chain gateway, e.g. http://127.0.0.1:4190
   * @param {number} [o.civ]    your civilization, once you have one
   * @param {Keypair} [o.session] your session key (signs your batches)
   */
  constructor({ server = 'http://127.0.0.1:4185', gateway = 'http://127.0.0.1:4190', civ = null, session = null } = {}) {
    this.server = server.replace(/\/$/, '');
    this.gateway = gateway.replace(/\/$/, '');
    this.civ = civ;
    this.session = session;
    this.decisions = new Map(); // tick -> commitment not yet known to be revealed
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

  // ------------------------------------------------------------ reading
  /** Seats: who plays which civilization (human, bot, external). */
  seats() { return this.get(this.server, '/api/seats'); }
  /** Static map: tiles `[q, r, terrain, river, resource]`, trade hubs. */
  map() { return this.get(this.server, '/api/map'); }
  /** Your fogged view (or `civ`'s). Includes `decision.obsRoot`, which your commitment binds to. */
  state(civ = this.civ) { return this.get(this.server, `/api/state${civ === null ? '' : `?civ=${civ}`}`); }
  /** kind: unit | city | research | diplomacy | path | amm. */
  preview(kind, params = {}, civ = this.civ) {
    const q = new URLSearchParams({ ...(civ === null ? {} : { civ: String(civ) }), ...Object.fromEntries(Object.entries(params).map(([k, v]) => [k, String(v)])) });
    return this.get(this.server, `/api/preview/${kind}?${q}`);
  }
  /**
   * Dry run: `{ok, cost, spendable, error?, warnings[]}`. `ok` is what the
   * chain checks at submit time; `warnings` are orders that would be skipped
   * when the tick resolves, judged from what you can see.
   */
  validate(orders, civ = this.civ) { return this.post(this.server, '/api/validate', { civ, orders }); }
  /** Committed and revealed decisions of past ticks. */
  decisionLog(limit = 120) { return this.get(this.server, `/api/decisions?limit=${limit}`); }
  /** Season account (entrants, pool, payouts) and the gateway's phase. */
  season() { return this.get(this.gateway, '/season'); }

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

  // ------------------------------------------------------------ entry (x402)
  /** Localnet only: a token account with 100 test USDC (no value) for `owner`. */
  faucet(owner) { return this.post(this.gateway, '/faucet', { owner: owner.toBase58() }); }

  /**
   * Pay the entry fee over HTTP 402 and join the season.
   * 1. POST /x402/join → 402 with PaymentRequirements (scheme "exact").
   * 2. Sign the program's JoinSeason (it moves exactly the entry fee from
   *    `usdcAccount` into the season vault) with `wallet`; the facilitator
   *    is the fee payer and adds its signature when it settles.
   * 3. POST again with X-PAYMENT → 200 + X-PAYMENT-RESPONSE.
   * @returns {{civ, name, signature, requirements, paymentResponse}}
   */
  async joinViaX402({ wallet, session, name, kind = 1, usdcAccount, payout = wallet.publicKey }) {
    const url = `${this.gateway}/x402/join`;
    const first = await http(url, { method: 'POST', body: { name } });
    if (first.status !== 402) throw new HttpError(first.status, first.body, url);
    const req = first.body.accepts?.[0];
    if (!req || req.scheme !== 'exact') throw new Error('gateway did not offer the "exact" scheme');
    const x = req.extra;
    const chain = new ChainClient(x.programId, BigInt(x.seasonId));
    // Never sign for a different season or vault than the requirements name.
    if (chain.season.toBase58() !== x.accounts.season || chain.vault.toBase58() !== req.payTo) throw new Error('payment requirements do not match the season PDAs');
    const tx = new Transaction();
    tx.add(...chain.joinSeason({ player: wallet.publicKey, feePayer: new PublicKey(x.feePayer), civ: x.civ, playerToken: new PublicKey(usdcAccount),
      mint: new PublicKey(req.asset), name, kind, session: session.publicKey, payout }));
    tx.feePayer = new PublicKey(x.feePayer);
    tx.recentBlockhash = x.recentBlockhash;
    tx.partialSign(wallet);
    const payment = { x402Version: 1, scheme: 'exact', network: req.network, payload: { transaction: tx.serialize({ requireAllSignatures: false }).toString('base64') } };
    const second = await http(url, { method: 'POST', body: { name }, headers: { 'X-PAYMENT': Buffer.from(JSON.stringify(payment)).toString('base64') } });
    if (second.status !== 200) throw new HttpError(second.status, second.body, url);
    const header = second.headers.get('x-payment-response');
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

  /**
   * Commit to this tick's decision, sign the batch with the session key and
   * relay it through the gateway (which only adds the fee payer).
   * Earlier decisions are revealed automatically (at most 3 per batch).
   * @param {object} o
   * @param {object[]} o.orders  order DTOs (see llms.txt)
   * @param {string} o.policy     short name of how you decide, e.g. "rule-agent/v1"
   * @param {string} [o.rationale] why (≤512 bytes); revealed after the tick resolves
   * @param {object} [o.view]     the state you decided from (saves a request)
   */
  async submit({ orders, policy, rationale = '', view = null }) {
    if (this.civ === null || !this.session) throw new Error('no seat: join first (joinViaX402) or pass civ and session');
    const v = view ?? await this.state();
    const tick = v.tick;
    if (!v.decision?.obsRoot) throw new Error('the game server has no observation root for this tick yet');
    const c = commit({ tick, obsRoot: v.decision.obsRoot, policy, text: rationale });
    // Reveal resolved decisions not yet known to be revealed, oldest first,
    // as many (up to 3) as fit next to the orders in one transaction.
    const reveals = [];
    let used = orders.reduce((n, o) => n + encodedLen(o), 0);
    for (const d of [...this.decisions.values()].filter(d => d.tick < tick && !(d.sentIn !== undefined && d.sentIn < tick)).sort((a, b) => a.tick - b.tick).slice(0, 3)) {
      const n = encodedLen(revealOrder(d));
      if (used + n > BATCH_BYTES) break;
      used += n;
      reveals.push(d);
    }
    if (used > BATCH_BYTES) return { ok: false, tick, error: `batch too large for one transaction (${used} > ${BATCH_BYTES} bytes of orders)` };
    const all = [...orders, ...reveals.map(revealOrder)];
    const check = await this.validate(all);
    if (!check.ok) return { ok: false, tick, error: check.error, warnings: check.warnings };
    const chain = await this.chainClient();
    const relay = await this.get(this.gateway, '/relay');
    const tx = new Transaction().add(...chain.submitOrders({ signer: this.session.publicKey, civ: this.civ, tick, decisionDigest: Buffer.from(c.digest, 'hex'), orders: all }));
    tx.feePayer = new PublicKey(relay.feePayer);
    tx.recentBlockhash = relay.blockhash;
    tx.partialSign(this.session);
    const r = await this.post(this.gateway, '/relay', { tx: tx.serialize({ requireAllSignatures: false }).toString('base64') });
    this.decisions.set(tick, c);
    for (const d of reveals) d.sentIn = tick;
    // Forget reveals that went out in a batch that has since resolved.
    for (const [t, d] of this.decisions) if (d.sentIn !== undefined && d.sentIn < tick) this.decisions.delete(t);
    return { ok: true, tick, cost: check.cost, warnings: check.warnings, digest: c.digest, revealed: reveals.map(d => d.tick), signature: r.signature };
  }
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
