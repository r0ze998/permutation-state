// The crank: keeps a season moving. Everything it does is permissionless or
// operator-only liveness; it cannot change outcomes (see permutation-chain
// DESIGN.md, "Trust model").
//
// * Registration: once the expected outside entrants joined (or at once),
//   start the season: genesis, seating, the first election, delegation.
// * Move the open tick through its sealed-orders phases on the ER: after
//   its deadline close the commitments (`CloseCommits`) and reveal the
//   batches this gateway holds for its hosted members (`RevealOrders`);
//   once every commitment is revealed or the reveal window ended, publish
//   the input (`LogTickInput`; chunk 0 freezes it and draws the randomness
//   from the revealed salts), resolve it (`ResolveTick`, in parts when it
//   does not fit one transaction) and archive every part's PS_TICK record.
// * Every `commitEvery` ticks, commit the ER state to the base layer in
//   small `CommitPart` intents (`intentGroups`).
// * After the last tick: `UndelegatePart` intents (commit and undelegate),
//   wait for every account to return to the base layer, run FinishSeason.
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { PublicKey } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { chainError, decodeNationHeader, decodeSeason, decodeWorldHeader, MAGIC, MAX_NATIONS, NATION_TARGET, roleIndex, WORLD_CHUNKS } from '../client/src/codec.mjs';
import { LOCAL_DIR, namedKey } from './config.mjs';
import { aiMembers, startAndDelegate } from './season.mjs';
import { TalkBook } from './talk.mjs';
import { records, send } from './send.mjs';
import { SealedStore } from './sealed.mjs';
import { appendTickLines, publishTickInput, readTickLines, resolveInParts, tickLinesOf } from './ticks.mjs';

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
/** Offices per nation. */
const OFFICES = 4;

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

export class Crank {
  /**
   * @param {object} o
   * @param {object} o.store   the season state store (config.mjs createStateStore); its `state` is shared with the routes
   * @param {string} [o.ticksDir] where the tick index (<season>.jsonl) is kept
   */
  constructor({ base, er, cfg, store, log = console.log, ticksDir = path.join(LOCAL_DIR, 'ticks') }) {
    Object.assign(this, { base, er, cfg, store, log });
    this.crank = namedKey('crank');
    this.chain = new ChainClient(cfg.programId, BigInt(this.state.seasonId));
    this.program = new PublicKey(cfg.programId);
    this.nations = null; // from the season account (`nationCount`)
    this.snapshot = null; // { world, header, nations: [...headers], slot, at, layer }
    this.busy = false;
    this.phase = this.state.finalized ? 'finalized' : this.state.delegated ? 'playing' : 'registering';
    this.openedAt = Date.now();
    this.reach = {};
    mkdirSync(ticksDir, { recursive: true });
    this.tickFile = path.join(ticksDir, `${this.state.seasonId}.jsonl`);
    /** Hosted members' sealed batches, revealed after each tick's close. */
    this.sealed = new SealedStore(path.join(ticksDir, `${this.state.seasonId}.sealed.json`));
    /** The CloseCommits transaction of a tick: {tick, signature}. */
    this.closed = null;
  }

  get state() { return this.store.state; }

  async season() {
    return decodeSeason((await this.base.getAccountInfo(this.chain.season, 'confirmed')).data);
  }

  /** Nations of the season, as its account says (read once). */
  async nationCount() {
    this.nations ??= (await this.season()).nations;
    return this.nations;
  }

  /** Latest world + nation headers from wherever the world lives now. */
  async refresh() {
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

  tickRecords(from = 0) {
    return readTickLines(this.tickFile, from);
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

  /** Registration: start once the expected outside entrants joined, or after the wait. */
  async registering() {
    const season = await this.season();
    const hosted = this.state.members.filter(m => m.hosted !== 'external').length;
    const external = season.memberCount - hosted;
    const waited = (Date.now() - this.openedAt) / 1000;
    const enough = external >= this.cfg.waitExternal;
    // --registration-seconds 0 means no time limit: wait for the entrants.
    const timedOut = this.cfg.registrationSeconds > 0 && waited >= this.cfg.registrationSeconds;
    if (!enough && !timedOut) return;
    this.log(`registration closes: ${season.memberCount} members (${external} from outside)`);
    await startAndDelegate({ base: this.base, er: this.er, cfg: this.cfg, store: this.store, log: this.log });
    this.phase = 'playing';
  }

  async play(snap) {
    const { meta } = snap.header;
    const nations = await this.nationCount();
    if (meta.finished) return this.undelegate(nations);
    const open = snap.nations[0]?.openTick;
    const { committed, revealed } = countSeals(snap.nations);
    const step = nextStep({ meta, openTick: open, committed, revealed, now: Date.now(), delegatedAt: this.state.delegatedAt, tickSeconds: this.cfg.tickSeconds });
    if (meta.revealing && !meta.frozen) await this.revealHosted(open, nations, snap.nations);
    if (!step) return;
    if (step === 'close') {
      try {
        const r = await send(this.er, this.chain.closeCommits({ nations }), [this.crank], `close tick ${open} commitments`);
        this.closed = { tick: open, signature: r.signature };
      } catch (e) {
        const code = chainError(e.message);
        if (code === 'TooEarly') { this.retryAt = Date.now() + 500; return; } // the ER clock has not reached the deadline yet
        if (code !== 'WrongPhase') throw e; // someone else closed it: go on from the next snapshot
      }
      this.retryAt = Date.now() + 300; // let the close land before revealing
      return;
    }
    // Publish the tick's input on chain first (PS_INPUT chunks; chunk 0
    // freezes it): the program resolves only a published input.
    let published;
    try {
      published = await this.publishInput(open, nations);
    } catch (e) {
      if (chainError(e.message) === 'TooEarly') { this.retryAt = Date.now() + 500; return; } // the reveal window is still open on the ER's clock
      throw e;
    }
    const commitSignature = this.closed?.tick === open ? this.closed.signature : await this.findClose(open);
    const seals = { committed, revealed, commitSignature };
    // From each cursor try the furthest stop that worked last time, then
    // shorter ones; every 10 ticks probe further again, since cost depends
    // on the world. Every part is archived as soon as it lands.
    if (open % 10 === 0) this.reach = {};
    const parts = await resolveInParts({
      reach: this.reach,
      resolve: to => send(this.er, this.chain.resolveTick({ nations, to }), [this.crank], `resolve tick ${open} to phase ${to}`),
      onPart: r => appendTickLines(this.tickFile, tickLinesOf(r, published, seals)),
    });
    this.sealed.dropBefore(open + 1);
    this.log(`tick ${open} resolved on ER (${revealed}/${committed} sealed batches revealed, ${OFFICES * nations} offices, ${parts.map(r => r.cu).join(' + ')} CU)`);
    if ((this.state.talk ?? []).some(m => !m.anchored)) {
      await this.anchorTalk(open + 1).catch(e => this.log(`talk anchor: ${e.message}`));
    }
    if (commitDue({ open, commitEvery: this.cfg.commitEvery, commits: this.state.commits })) {
      for (const group of intentGroups({ nations })) {
        await send(this.er, this.chain.commitPart({ payer: this.crank.publicKey, targets: group }), [this.crank], `commit ${group.join(',')}`);
      }
      this.state.commits = (this.state.commits ?? 0) + 1;
      this.store.save();
      this.log(`requested an ER→base commit at tick ${open + 1} (the ER validator settles it on base)`);
    }
  }

  /**
   * Reveal the batches this gateway sealed for its hosted members (the
   * reveal window is open). Offices already revealed are skipped, so it is
   * safe to call on every snapshot; a failed reveal is logged, not retried
   * forever (it is retried on the next snapshot while the window is open).
   */
  async revealHosted(tick, nations, headers) {
    // Only the kept batches whose commitment is the one on chain.
    const due = this.sealed.forTick(tick).filter(b => {
      const h = headers[b.civ], i = roleIndex(b.role);
      return h && h.submitted[i] !== tick && h.committed[i] === tick && Buffer.from(h.commits[i]).toString('hex') === b.commitment;
    });
    // All at once: the window is a few seconds and each reveal is a round
    // trip (~0.4 s to a public ER), so one after another the last nations
    // would miss it.
    await Promise.all(due.map(b =>
      send(this.er, this.chain.revealOrders({ signer: this.crank.publicKey, ...b }), [this.crank], `reveal ${b.civ} ${b.role} tick ${tick}`)
        .catch(e => this.log(`reveal ${b.civ} ${b.role} tick ${tick}: ${chainError(e.message) ?? e.message}`))));
  }

  /**
   * The CloseCommits transaction of `tick` when someone else sent it: the
   * latest transactions on world chunk 0 with a PS_COMMITS record for it.
   */
  async findClose(tick) {
    const sigs = await this.er.getSignaturesForAddress(this.chain.worldChunks[0], { limit: 40 }, 'confirmed');
    for (const { signature, err } of sigs) {
      if (err) continue;
      const t = await this.er.getTransaction(signature, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 });
      if (records(t?.meta?.logMessages ?? []).some(r => r.tag === 'PS_COMMITS' && r.tick === tick)) return signature;
    }
    this.log(`tick ${tick}: its CloseCommits transaction was not found; the verifier cannot check its reveals`);
    return null;
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
      await send(this.base, this.chain.revealRoster({ members: group.map(m => new PublicKey(m.wallet)), salts: group.map(m => Uint8Array.from(Buffer.from(m.salt, 'hex'))) }),
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
      for (const m of messages) m.anchored = { signature: r.signature, root: Buffer.from(root).toString('hex') };
    }
    this.store.save();
  }
}

/** Operator AI members revealed per `RevealRoster` transaction (a member account each). */
export const ROSTER_BATCH = 8;

/**
 * A `PS_HISTORY` record as kept in the state file, which is JSON: u64s as
 * decimal strings, bytes as hex.
 */
export function historyEntry(h, signature) {
  const json = v => JSON.parse(JSON.stringify(v, (_, x) => (typeof x === 'bigint' ? x.toString() : x instanceof Uint8Array ? Buffer.from(x).toString('hex') : x)));
  return json({ prevHistoryRoot: h.prevHistoryRoot, historyRoot: h.historyRoot, record: h.record, signature });
}
