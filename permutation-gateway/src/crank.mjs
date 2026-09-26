// The crank: keeps a season moving. Everything it does is permissionless or
// operator-only liveness; it cannot change outcomes (see permutation-chain
// DESIGN.md, "Trust model").
//
// * Registration: fund and register the operator's AI members at their
//   planned times, strictly in roster order (season.mjs `planAi`), paused
//   like people's joins while the crank is low on SOL (the FundsGuard the
//   routes share, guards.mjs); when the
//   window closes (dev mode: once enough other members joined) and every AI
//   member is registered, start the season: genesis, seating, the first
//   election, delegation.
// * Move the open tick through its sealed-orders phases on the ER: after
//   its deadline close the commitments (`CloseCommits`) and reveal the
//   batches this gateway holds (its AI members' and those deposited through
//   POST /seal, `RevealOrders`);
//   once every commitment is revealed or the reveal window ended, publish
//   the input (`LogTickInput`; chunk 0 freezes it and draws the randomness
//   from the revealed salts), resolve it (`ResolveTick`, in parts when it
//   does not fit one transaction) and archive every part's PS_TICK record.
// * Fill tick-index gaps left by others' ResolveTick parts from the ER
//   history, in the background (one bounded attempt per gap); a tick
//   closed by someone else has commitSignature null. Nothing that reads
//   history runs on the path that closes, reveals and resolves.
// * Every `commitEvery` ticks, commit the ER state to the base layer in
//   small `CommitPart` intents (`intentGroups`).
// * After the last tick: `UndelegatePart` intents (commit and undelegate),
//   wait for every account to return to the base layer, run FinishSeason.
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { PublicKey } from '@solana/web3.js';
import { fromHex, jsonSafe, toHex } from '../client/src/bytes.mjs';
import { ChainClient, readSeason } from '../client/src/chain.mjs';
import { chainError, decodeNationHeader, decodeWorldHeader, MAGIC, MAX_NATIONS, NATION_TARGET, NATIONS, roleIndex, ROLES, WORLD_CHUNKS } from '../client/src/codec.mjs';
import { LOCAL_DIR, namedKey } from './config.mjs';
import { registrationCost } from './faucet.mjs';
import { aiMembers, fundPlanned, registerPlanned, registrationOf, respread, RESPREAD_LATE_MS, startAndDelegate, unit } from './season.mjs';
import { TalkBook } from './talk.mjs';
import { records, send } from './send.mjs';
import { SealedStore } from './sealed.mjs';
import { appendTickLines, LAST_PHASE, publishTickInput, readTickLines, resolveInParts, tickLinesOf } from './ticks.mjs';
import { buildTickLines, chainLines, scanRecords } from './tickscan.mjs';

/** The program judges deadlines by the ER's clock, which can trail ours by a second. */
export const DEADLINE_GRACE_S = 1;
/**
 * The ER sponsors at most 10 commits per delegated account (after that it
 * asks for a delegated fee payer). One is kept for the final
 * commit-and-undelegate, so periodic commits stop at 9.
 */
export const SPONSORED_COMMITS = 10;
/** Nation accounts per intent (see `intentGroups`). */
export const INTENT_GROUP = 3;
/** Targets one `CommitPart`/`UndelegatePart` may name (the program's limit). */
export const MAX_INTENT_TARGETS = WORLD_CHUNKS + MAX_NATIONS;

/**
 * The season's accounts in small intents, for commits and undelegation.
 * One intent's base-layer finalize runs every target in one transaction,
 * so intents must stay small (measured on devnet): 14 accounts in one
 * intent exceed 64 account keys (magicblock-validator#1693) and Solana's
 * instruction trace, and several densely written world chunks exceed the
 * finalize's compute budget, leaving them stuck undelegating. So: nation
 * accounts in groups of `groupSize`, each world chunk alone, chunk 0 (its
 * header says whether the season is over) last.
 */
export function intentGroups({ nations, chunks = WORLD_CHUNKS, groupSize = INTENT_GROUP }) {
  const nationTargets = Array.from({ length: nations }, (_, c) => NATION_TARGET + c);
  const groups = [];
  for (let i = 0; i < nationTargets.length; i += groupSize) groups.push(nationTargets.slice(i, i + groupSize));
  for (let k = 1; k < chunks; k++) groups.push([k]);
  groups.push([0]);
  return groups;
}

/** Offices (over all nations) that sealed orders for their nation's open tick, and how many of those were revealed. */
export const countSeals = nationHeaders => nationHeaders.reduce((n, x) => {
  if (!x) return n;
  n.committed += x.committed.filter(t => t === x.openTick).length;
  n.revealed += x.submitted.filter(t => t === x.openTick).length;
  return n;
}, { committed: 0, revealed: 0 });

/**
 * What the open tick needs next:
 * - `close`: the commitments are open and the deadline (plus a grace second
 *   for the ER's clock) passed. Tick 0's deadline was set on base when the
 *   government opened and delegation takes a few seconds, so tick 0 gets one
 *   full tick on the ER first. Closing never waits for the officers: members
 *   without office use the whole tick for proposals, support and votes.
 * - `publish`: in the reveal window, every commitment was revealed or the
 *   reveal deadline passed; or the input is already frozen.
 * - `null`: wait.
 */
export function nextStep({ meta, openTick, committed, revealed, now, delegatedAt = 0, tickSeconds }) {
  if (meta.frozen) return 'publish';
  const passed = now / 1000 >= meta.deadline + DEADLINE_GRACE_S;
  if (meta.revealing) return revealed >= committed || passed ? 'publish' : null;
  const settling = openTick === 0 && now < delegatedAt + tickSeconds * 1000;
  return !settling && passed ? 'close' : null;
}

/** Whether to request a periodic commit after resolving tick `open`. */
export const commitDue = ({ open, commitEvery, commits = 0 }) =>
  commitEvery > 0 && (open + 1) % commitEvery === 0 && commits < SPONSORED_COMMITS - 1;

/** Pages (of 1000 transactions) the background indexer reads for one gap. */
export const INDEX_PAGES = 1;

/** A tick line's identity in the index: its transaction and the root it starts from. */
const lineKey = l => `${l.signature}:${l.preRoot}`;
/** A gap's identity: after which line, before which line (or up to which open tick). */
const gapKey = g => `${g.after?.signature ?? 'open'}|${g.before?.signature ?? `tail@${g.openTick}`}`;
/** Index lines in chain order: within a tick, `to` grows along the chain. */
const chainOrder = (a, b) => a.tick - b.tick || a.to - b.to;

/** Operator AI members revealed per `RevealRoster` transaction (a member account each). */
export const ROSTER_BATCH = 8;

/**
 * A `PS_HISTORY` record as kept in the state file, which is JSON: u64s as
 * decimal strings, bytes as hex.
 */
export function historyEntry(h, signature) {
  return JSON.parse(JSON.stringify({ prevHistoryRoot: h.prevHistoryRoot, historyRoot: h.historyRoot, record: h.record, signature }, jsonSafe));
}

/**
 * Registrations being sent right now (x402 joins and the crank's own), by
 * session key and wallet: a second one with either is refused while the
 * first is in flight (a duplicate session key would not be seen on chain
 * yet), and the season does not start while one is.
 */
export class RegistrationDesk {
  constructor() {
    this.sessions = new Set();
    this.wallets = new Set();
  }

  get busy() { return this.sessions.size > 0; }
  get size() { return this.sessions.size; }

  /** Reserve (base58 keys); returns a release function, or null if either is taken. */
  reserve({ wallet, session }) {
    if (this.sessions.has(session) || this.wallets.has(wallet)) return null;
    this.sessions.add(session);
    this.wallets.add(wallet);
    let done = false;
    return () => {
      if (done) return;
      done = true;
      this.sessions.delete(session);
      this.wallets.delete(wallet);
    };
  }
}

/** An AI member's blockhash is fetched this long (ms) before it registers, drawn per member. */
export const AI_BLOCKHASH_AGE_MS = Object.freeze({ min: 3000, max: 40_000 });
/** A fetched blockhash older than this is fetched again (it must still land). */
const BLOCKHASH_STALE_MS = 50_000;
/** How often (ms) the crank repeats that registration closed with AI members still unregistered. */
const LATE_LOG_MS = 30_000;

export class Crank {
  /**
   * @param {object} o
   * @param {object} o.store   the season state store (config.mjs createStateStore); its `state` is shared with the routes
   * @param {string} [o.ticksDir] where the tick index (<season>.jsonl) is kept
   * @param {Function} [o.keys] named key loader (config.mjs namedKey)
   * @param {Function} [o.now] the clock (ms) registration runs by
   * @param {object} [o.funds] the FundsGuard the routes share (guards.mjs): while it says the
   *                           crank is low, the AI members are neither funded nor registered
   */
  constructor({ base, er, cfg, store, log = console.log, ticksDir = path.join(LOCAL_DIR, 'ticks'), keys = namedKey, now = Date.now, funds = null }) {
    Object.assign(this, { base, er, cfg, store, log, keys, now, funds });
    this.crank = keys('crank');
    this.chain = new ChainClient(cfg.programId, BigInt(this.state.seasonId));
    this.program = new PublicKey(cfg.programId);
    this.nations = null; // from the season account (`nationCount`)
    this.snapshot = null; // { world, header, nations: [...headers], slot, at, layer }
    this.busy = false;
    this.phase = this.state.finalized ? 'finalized' : this.state.delegated ? 'playing' : 'registering';
    this.startedAt = now();
    this.reach = {};
    /** Registrations in flight (shared with /x402/join). */
    this.desk = new RegistrationDesk();
    /** The next AI member's blockhash, fetched ahead: {pos, at, value, sendAt}. */
    this.aiBlockhash = null;
    this.aiAges = new Map();
    /** How a registration whose confirmation failed is looked for on chain. */
    this.recheck = { attempts: 4, delayMs: 1000 };
    this.refreshing = null;
    mkdirSync(ticksDir, { recursive: true });
    this.tickFile = path.join(ticksDir, `${this.state.seasonId}.jsonl`);
    /** Sealed batches (AI members', and deposits through POST /seal), revealed after each tick's close. */
    this.sealed = new SealedStore(path.join(ticksDir, `${this.state.seasonId}.sealed.json`));
    /** The CloseCommits transaction of a tick: {tick, signature}. */
    this.closed = null;
    /**
     * The tick index as this crank keeps it: the keys of its lines (loaded
     * from the file on first use), the line the chain is at, the gaps to
     * fill in the background (each tried once) and the scan in flight.
     */
    this.indexed = null;
    this.head = null;
    this.gaps = [];
    this.triedGaps = new Set();
    this.indexing = null;
    this.slots = new Map(); // signature → slot, as scans saw them
  }

  get state() { return this.store.state; }

  season() { return readSeason(this.base, this.chain); }

  /** Nations of the season, as its account says (read once). */
  async nationCount() {
    this.nations ??= (await this.season()).nations;
    return this.nations;
  }

  /** The registration window (season.mjs `registrationOf`; a season from before windows: dev mode from this crank's start). */
  registration() {
    const reg = registrationOf(this.state, { fallbackOpenedAt: this.startedAt, entryFee: this.cfg.entryFee });
    return this.state.registration ? reg : { ...reg, waitExternal: this.cfg.waitExternal ?? 0 };
  }

  /** Latest world + nation headers from wherever the world lives now; concurrent callers share one read. */
  refresh() {
    this.refreshing ??= this.readWorld().finally(() => { this.refreshing = null; });
    return this.refreshing;
  }

  /** The snapshot if it is at most `maxAgeMs` old, else a fresh one. */
  async fresh(maxAgeMs) {
    return this.snapshot && Date.now() - this.snapshot.at < maxAgeMs ? this.snapshot : this.refresh();
  }

  async readWorld() {
    const conn = this.phase === 'playing' ? this.er : this.base;
    const chunks = this.chain.worldChunks.length;
    const keys = [...this.chain.worldChunks, ...this.chain.nations(await this.nationCount())];
    const res = await conn.getMultipleAccountsInfoAndContext(keys, 'confirmed');
    const worldParts = res.value.slice(0, chunks);
    const nations = res.value.slice(chunks);
    if (worldParts.some(w => !w)) return null;
    // Reassemble: header + body continue across the chunks in order.
    const world = Buffer.concat(worldParts.map(w => w.data));
    const header = decodeWorldHeader(world);
    if (header.magic !== MAGIC.world) return null; // genesis still running
    this.snapshot = {
      world,
      header,
      nations: nations.map(n => { try { return n ? decodeNationHeader(n.data) : null; } catch { return null; } }),
      slot: res.context.slot,
      at: Date.now(),
      layer: this.phase === 'playing' ? 'er' : 'base',
    };
    return this.snapshot;
  }

  /** The tick index from `from`, in chain order (a filled gap lands after the lines past it). */
  tickRecords(from = 0) {
    return readTickLines(this.tickFile, from).sort(chainOrder);
  }

  /** Load the index's keys and head from the file, once. */
  loadIndex() {
    if (this.indexed) return;
    const lines = this.tickRecords();
    this.indexed = new Set(lines.map(lineKey));
    this.head = lines.at(-1) ?? null;
  }

  /** Queue a gap unless it is queued or was tried already. */
  queueGap(gap) {
    const k = gapKey(gap);
    if (!this.triedGaps.has(k) && !this.gaps.some(g => gapKey(g) === k)) this.gaps.push(gap);
  }

  /**
   * Archive a landed resolve part at once: one line per advancing PS_TICK
   * (no-op parts, `ResolveTick { to <= cursor }` after someone else's part,
   * are skipped), never twice. A part that does not start from the head's
   * root came after something the index lacks: that gap is queued.
   */
  archivePart(result, published, seals) {
    this.loadIndex();
    for (const line of tickLinesOf(result, published, seals)) {
      if (line.preRoot === line.root || this.indexed.has(lineKey(line))) continue;
      appendTickLines(this.tickFile, [line]);
      this.indexed.add(lineKey(line));
      const from = this.head?.root ?? this.state.open?.root;
      if (from && line.preRoot !== from) this.queueGap({ after: this.head, before: line });
      this.head = line;
    }
  }

  /** Queue a tail gap when the open tick is past what the index reaches (others resolved whole ticks). */
  noteTail(openTick) {
    this.loadIndex();
    const h = this.head;
    const expected = h ? (h.to >= LAST_PHASE ? h.tick + 1 : h.tick) : 0;
    if (Number.isInteger(openTick) && openTick > expected) this.queueGap({ after: h, before: null, openTick });
  }

  /**
   * Fill the oldest untried gap from world chunk 0's ER history, in the
   * background: single-flight, never awaited by `step()`, one attempt per
   * gap. Returns the scan in flight (tests await it).
   */
  kickIndexer() {
    if (this.indexing) return this.indexing;
    const gap = this.gaps.shift();
    if (!gap) return Promise.resolve();
    this.triedGaps.add(gapKey(gap));
    this.indexing = this.fillGap(gap)
      .catch(e => this.log(`tick index: gap after tick ${gap.after?.tick ?? 'open'}: ${e.message}`))
      .finally(() => { this.indexing = null; });
    return this.indexing;
  }

  /** The slot of a transaction (from a scan, else its status). */
  async slotOf(signature) {
    if (!this.slots.has(signature)) {
      const st = await this.er.getSignatureStatuses([signature], { searchTransactionHistory: true });
      this.slots.set(signature, st?.value?.[0]?.slot ?? 0);
    }
    return this.slots.get(signature);
  }

  async fillGap({ after, before, openTick }) {
    const chunk0 = this.chain.worldChunks[0];
    const lo = after ? await this.slotOf(after.signature) : 0;
    const { txs } = await scanRecords({ connection: this.er, address: chunk0, program: this.cfg.programId, lo, start: before?.signature ?? null, maxPages: INDEX_PAGES, concurrency: 4 });
    for (const t of txs) this.slots.set(t.signature, t.slot);
    const from = after?.root ?? this.state.open?.root;
    const found = chainLines(buildTickLines(txs, { anchor: chunk0, knownLines: after ? [after] : [] }), from);
    const fresh = found.filter(l => !this.indexed.has(lineKey(l)));
    appendTickLines(this.tickFile, fresh);
    for (const l of fresh) this.indexed.add(lineKey(l));
    const last = found.at(-1);
    if (!before && last && (!this.head || chainOrder(last, this.head) > 0)) this.head = last;
    const filled = before ? (found.length ? last.root : from) === before.preRoot : found.length > 0;
    if (!filled) {
      this.log(`index gap after tick ${after?.tick ?? 'open'}${before ? '' : ` (to open tick ${openTick})`} not filled within ${INDEX_PAGES * 1000} transactions; the verifier recovers it from the ER history; scripts/reindex-ticks.mjs rebuilds the index`);
    }
  }

  async step() {
    if (this.busy || this.phase === 'finalized' || Date.now() < (this.retryAt || 0)) return;
    this.busy = true;
    try {
      if (this.phase === 'registering') return await this.registering();
      const snap = await this.refresh();
      if (!snap && this.phase !== 'settling') return;
      if (this.phase === 'playing') await this.play(snap);
      else if (this.phase === 'settling') await this.settle();
    } catch (e) {
      this.log(`crank: ${e.message}${e.logs?.length ? `\n  ${e.logs.slice(-4).join('\n  ')}` : ''}`);
      this.retryAt = Date.now() + 3000; // back off instead of hammering a failing step
    } finally {
      this.busy = false;
    }
  }

  /**
   * Registration: the AI members' funding and registrations as planned;
   * then, once the window closed (dev mode: enough other members joined),
   * every AI member is registered and no registration is in flight, start.
   * One chain action per step.
   */
  async registering() {
    const season = await this.season();
    // Started already (a restart during genesis or seating): go on.
    if (season.status !== 'Registering') return this.start(season);
    const reg = this.registration();
    const now = this.now();
    const pending = (this.state.aiPlan ?? []).filter(e => e.registered === undefined).sort((a, b) => a.pos - b.pos);
    if (pending.length) return this.registerAis(pending, reg, now, season);
    const others = season.memberCount - this.state.members.filter(m => m.hosted === 'ai').length;
    const due = reg.closesAt !== null ? now >= reg.closesAt : others >= reg.waitExternal;
    if (!due || this.desk.busy) return;
    this.log(`registration closes: ${season.memberCount} members (${others} besides the operator's AI members)`);
    return this.start(season);
  }

  async start() {
    await startAndDelegate({ base: this.base, er: this.er, cfg: this.cfg, store: this.store, log: this.log });
    this.phase = 'playing';
  }

  /**
   * The next step for the AI members not registered yet (`pending`, in
   * `pos` order): fund one whose funding time came; else, for the first of
   * them (strictly in order), fetch its blockhash a drawn 3–40 s ahead and
   * register it once due. Overdue ones are spread over the rest of the
   * window first.
   */
  async registerAis(pending, reg, now, season) {
    // Low on SOL: people's joins and faucet grants are paused (503
    // OperatorLowFunds), so the AI members wait too; otherwise the only
    // registrations landing would be theirs. Overdue ones are spread again
    // once it is funded.
    if (await this.lowFunds()) return;
    const cost = registrationCost(reg.entryFee ?? season.entryFee, reg.deposit);
    const next = pending[0];
    if (reg.seconds && next.dueAt < now - RESPREAD_LATE_MS) {
      const n = respread(this.state.aiPlan, { now, openedAt: reg.openedAt, seconds: reg.seconds, closesAt: reg.closesAt });
      this.store.save();
      this.aiBlockhash = null;
      this.log(`${n} AI member registrations were overdue: spread over the rest of the window`);
      return;
    }
    const fund = pending.find(e => !e.funded && e.fundAt <= now);
    if (fund) {
      await fundPlanned({ base: this.base, store: this.store, entry: fund, amount: cost, keys: this.keys });
      return;
    }
    if (reg.closesAt !== null && now >= reg.closesAt && now - (this.lateLogAt ?? 0) >= LATE_LOG_MS) {
      this.lateLogAt = now;
      this.log(`WARNING: registration closed, but ${pending.length} AI members are not registered yet; the season starts once they are (retrying)`);
    }
    let bh = this.aiBlockhash?.pos === next.pos && now - this.aiBlockhash.at < BLOCKHASH_STALE_MS ? this.aiBlockhash : null;
    if (!bh) {
      // A person's Register carries the 402's blockhash, aged by the wallet's
      // approval; an AI member's is aged too.
      if (!this.aiAges.has(next.pos)) this.aiAges.set(next.pos, AI_BLOCKHASH_AGE_MS.min + (AI_BLOCKHASH_AGE_MS.max - AI_BLOCKHASH_AGE_MS.min) * unit());
      const age = reg.seconds ? this.aiAges.get(next.pos) : 0;
      if (now < next.dueAt - age) return;
      const value = await this.base.getLatestBlockhash('confirmed');
      this.aiBlockhash = { pos: next.pos, at: now, value, sendAt: Math.max(next.dueAt, now + (reg.seconds ? AI_BLOCKHASH_AGE_MS.min : 0)) };
      return;
    }
    if (now < bh.sendAt) return;
    if (!next.funded) {
      await fundPlanned({ base: this.base, store: this.store, entry: next, amount: cost, keys: this.keys });
      return;
    }
    try {
      const index = await registerPlanned({ base: this.base, cfg: this.cfg, store: this.store, entry: next, blockhash: bh.value, keys: this.keys, log: this.log, recheck: this.recheck });
      this.log(`member ${index} joined ${NATIONS[next.civ] ?? next.civ}`);
    } catch (e) {
      this.log(`WARNING: an AI member (roster position ${next.pos}) could not register: ${e.message}; retrying (the season starts only once every AI member is registered)`);
      throw e;
    } finally {
      this.aiBlockhash = null;
    }
  }

  /** Whether the shared FundsGuard says the crank is below its minimum SOL (no guard: never). */
  async lowFunds() {
    try {
      await this.funds?.check();
      return false;
    } catch (e) {
      if (e.code === 'OperatorLowFunds') return true;
      throw e;
    }
  }

  async play(snap) {
    try {
      // The index is off the critical path: a bad index file is logged, never a stall.
      try { this.noteTail(snap.nations[0]?.openTick); } catch (e) { this.log(`tick index: ${e.message}`); }
      return await this.playStep(snap);
    } finally {
      void this.kickIndexer();
    }
  }

  async playStep(snap) {
    const { meta } = snap.header;
    const nations = await this.nationCount();
    if (meta.finished) return this.undelegate(nations);
    const open = snap.nations[0]?.openTick;
    const { committed, revealed } = countSeals(snap.nations);
    const step = nextStep({ meta, openTick: open, committed, revealed, now: Date.now(), delegatedAt: this.state.delegatedAt, tickSeconds: this.cfg.tickSeconds });
    if (meta.revealing && !meta.frozen) await this.revealHosted(open, nations, snap.nations);
    if (!step) return;
    if (step === 'close') return this.closeTick(open, nations, snap);
    if (!(await this.publishAndResolve(open, nations, { committed, revealed }))) return;
    if ((this.state.talk ?? []).some(m => !m.anchored)) {
      await this.anchorTalk(open + 1).catch(e => this.log(`talk anchor: ${e.message}`));
    }
    await this.maybeCommit(open, nations);
  }

  /** Close the open tick's commitments (`CloseCommits`) and reveal the kept batches at once. */
  async closeTick(open, nations, snap) {
    try {
      const r = await send(this.er, this.chain.closeCommits({ nations }), [this.crank], `close tick ${open} commitments`);
      this.closed = { tick: open, signature: r.signature };
      // Reveal at once: the window is a few seconds, and the commitments
      // in this snapshot are the ones the close just sealed.
      await this.revealHosted(open, nations, snap.nations);
      return;
    } catch (e) {
      const code = chainError(e.message);
      if (code === 'TooEarly') { this.retryAt = Date.now() + 500; return; } // the ER clock has not reached the deadline yet
      if (code !== 'WrongPhase') throw e; // someone else closed it: go on from the next snapshot
    }
    this.retryAt = Date.now() + 300; // let the close land before revealing
  }

  /**
   * Publish the open tick's input, resolve it (in parts) and archive every
   * part. `seals` = {committed, revealed} of the snapshot. Returns whether
   * the tick was resolved (false: the reveal window is still open on the
   * ER's clock; try again shortly).
   */
  async publishAndResolve(open, nations, { committed, revealed }) {
    // Publish the tick's input on chain first (PS_INPUT chunks; chunk 0
    // freezes it): the program resolves only a published input.
    let published;
    try {
      published = await this.publishInput(open, nations);
    } catch (e) {
      if (chainError(e.message) === 'TooEarly') { this.retryAt = Date.now() + 500; return false; } // the reveal window is still open on the ER's clock
      throw e;
    }
    // Closed by someone else: null (the verifier finds this season's close itself).
    const commitSignature = this.closed?.tick === open ? this.closed.signature : null;
    const seals = { committed, revealed, commitSignature };
    // From each cursor try the furthest stop that worked last time, then
    // shorter ones; every 10 ticks probe further again, since cost depends
    // on the world. Every part is archived as soon as it lands.
    if (open % 10 === 0) this.reach = {};
    const parts = await resolveInParts({
      reach: this.reach,
      resolve: to => send(this.er, this.chain.resolveTick({ nations, to }), [this.crank], `resolve tick ${open} to phase ${to}`),
      onPart: r => { try { this.archivePart(r, published, seals); } catch (e) { this.log(`tick index: ${e.message}`); } },
    });
    this.sealed.dropBefore(open + 1);
    this.log(`tick ${open} resolved on ER (${revealed}/${committed} sealed batches revealed, ${ROLES.length * nations} offices, ${parts.map(r => r.cu).join(' + ')} CU)`);
    return true;
  }

  /** After resolving tick `open`: every `commitEvery` ticks, request an ER→base commit in small intents. */
  async maybeCommit(open, nations) {
    if (!commitDue({ open, commitEvery: this.cfg.commitEvery, commits: this.state.commits })) return;
    for (const group of intentGroups({ nations })) {
      await send(this.er, this.chain.commitPart({ payer: this.crank.publicKey, targets: group }), [this.crank], `commit ${group.join(',')}`);
    }
    this.state.commits = (this.state.commits ?? 0) + 1;
    this.store.save();
    this.log(`requested an ER→base commit at tick ${open + 1} (the ER validator settles it on base)`);
  }

  /**
   * Reveal the batches this gateway keeps (sealed.mjs: its AI members' and
   * the ones deposited through POST /seal; the reveal window is open), every
   * one signed by the crank alone. Offices already revealed are skipped, so
   * it is safe to call on every snapshot; a failed reveal is logged, not
   * retried forever (it is retried on the next snapshot while the window is
   * open).
   */
  async revealHosted(tick, nations, headers) {
    // Only the kept batches whose commitment is the one on chain.
    const due = this.sealed.forTick(tick).filter(b => {
      const h = headers[b.civ], i = roleIndex(b.role);
      return h && h.submitted[i] !== tick && h.committed[i] === tick && toHex(h.commits[i]) === b.commitment;
    });
    // All at once: the window is a few seconds and each reveal is a round
    // trip (~0.4 s to a public ER), so one after another the last nations
    // would miss it.
    await Promise.all(due.map(b =>
      send(this.er, this.chain.revealOrders({ signer: this.crank.publicKey, ...b }), [this.crank], `reveal ${b.civ} ${b.role} tick ${tick}`)
        .catch(e => this.log(`reveal ${b.civ} ${b.role} tick ${tick}: ${chainError(e.message) ?? e.message}`))));
  }

  /** The season's `PS_HISTORY` record and its FinishSeason transaction on base, if found. */
  async findHistory() {
    const sigs = await this.base.getSignaturesForAddress(this.chain.season, { limit: 40 }, 'confirmed');
    for (const { signature, err } of sigs) {
      if (err) continue;
      const t = await this.base.getTransaction(signature, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 });
      const record = records(t?.meta?.logMessages ?? []).find(r => r.tag === 'PS_HISTORY');
      if (record) return { record, signature };
    }
    this.log('the season\'s FinishSeason transaction was not found; GET /history has no record');
    return null;
  }

  /** After the last tick: commit and undelegate every account, chunk 0 last; resumable. */
  async undelegate(nations) {
    this.log('last tick resolved: committing and undelegating');
    const groups = intentGroups({ nations });
    const done = new Set(this.state.undelegated ?? []);
    for (const group of groups) {
      if (group.every(t => done.has(t))) continue;
      await send(this.er, this.chain.undelegatePart({ payer: this.crank.publicKey, targets: group }), [this.crank], `undelegate ${group.join(',')}`);
      for (const t of group) done.add(t);
      this.state.undelegated = [...done];
      this.store.save();
    }
    this.log(`undelegation scheduled for ${groups.flat().length} accounts in ${groups.length} intents`);
    this.phase = 'settling';
  }

  /**
   * Log every chunk of the open tick's input and return the reassembled
   * input, checked against the hash the program logged (cached per tick).
   */
  async publishInput(tick, nations) {
    if (this.published?.tick === tick) return this.published;
    this.published = await publishTickInput({
      tick,
      publishChunk: (chunk, total) => send(this.er, this.chain.logTickInput({ nations, chunk }), [this.crank], `publish tick ${tick} input ${chunk + 1}/${total}`),
    });
    return this.published;
  }

  async settle() {
    // Every chunk and nation account must be back on the base layer.
    const all = await this.base.getMultipleAccountsInfo([...this.chain.worldChunks, ...this.chain.nations(await this.nationCount())], 'confirmed');
    if (all.some(a => !a || !a.owner.equals(this.program))) return; // still undelegating
    const season = await this.season();
    if (season.status === 'Running') {
      // Operator AI members (V5 §18.2): reveal the roster first, in the
      // committed order, a few members per transaction.
      if (season.aiCount > 0 && season.rosterRevealed < season.aiCount) await this.revealRoster(season.rosterRevealed);
      const r = await send(this.base, this.chain.finishSeason({ roster: season.aiCount > 0 }), [this.crank], 'finishSeason');
      this.log('season finalized on base: every member\'s payout computed on chain');
      // The history layer: keep the season's record for GET /history.
      const h = r.records.find(x => x.tag === 'PS_HISTORY');
      if (h) {
        this.state.history = historyEntry(h, r.signature);
        this.log(`history root ${this.state.history.historyRoot.slice(0, 16)}… (${h.record.cities.length} cities, ${h.record.ruins.length} ruins)`);
      }
    }
    // A restart between FinishSeason and the save: read the record back.
    if (season.status !== 'Running' && !this.state.history) {
      const found = await this.findHistory();
      if (found) this.state.history = historyEntry(found.record, found.signature);
    }
    if (this.state.roster) this.state.roster.revealed = season.aiCount > 0;
    this.phase = 'finalized';
    this.state.finalized = true;
    this.store.save();
  }

  /** Reveal the operator AI members from position `from` of the roster (V5 §18.2). */
  async revealRoster(from) {
    const ais = aiMembers(this.state);
    for (let i = from; i < ais.length; i += ROSTER_BATCH) {
      const group = ais.slice(i, i + ROSTER_BATCH);
      await send(this.base, this.chain.revealRoster({ members: group.map(m => new PublicKey(m.wallet)), salts: group.map(m => fromHex(m.salt)) }),
        [this.crank], `reveal roster ${i}..${i + group.length - 1}`);
    }
    this.log(`operator AI roster revealed: ${ais.length} members (${ais.map(m => m.index).join(', ')})`);
  }

  /**
   * Anchor the members' messages of every tick before `open` not anchored
   * yet (V5 §18.7): one `AnchorTalk` per tick with messages.
   */
  async anchorTalk(open) {
    const book = new TalkBook(this.state);
    for (const [tick, messages] of book.pending(open)) {
      const root = TalkBook.root(messages);
      const r = await send(this.er, this.chain.anchorTalk({ crank: this.crank.publicKey, tick, count: messages.length, root }), [this.crank], `anchor tick ${tick} talk`);
      for (const m of messages) m.anchored = { signature: r.signature, root: toHex(root) };
      // Saved per tick: a later tick failing must not lose this one's
      // anchor (it would be anchored twice, with two PS_TALK roots).
      this.store.save();
    }
  }
}
