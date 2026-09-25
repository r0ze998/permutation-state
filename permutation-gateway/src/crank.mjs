// The crank: keeps a season moving. Everything it does is permissionless or
// operator-only liveness; it cannot change outcomes (see permutation-chain
// DESIGN.md, "Trust model").
//
// * Registration: once the expected outside entrants joined (or at once),
//   start the season: genesis, seating, the first election, delegation.
// * Resolve the open tick on the ER once it is due (its deadline passed, or
//   every office of every nation submitted): publish its input
//   (`LogTickInput`), resolve it (`ResolveTick`, in parts when it does not
//   fit one transaction) and archive every part's PS_TICK record.
// * Every `commitEvery` ticks, commit the ER state to the base layer in
//   small `CommitPart` intents (`intentGroups`).
// * After the last tick: `UndelegatePart` intents (commit and undelegate),
//   wait for every account to return to the base layer, run FinishSeason.
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { PublicKey } from '@solana/web3.js';
import { ChainClient } from '../client/src/chain.mjs';
import { chainError, decodeNationHeader, decodeSeason, decodeWorldHeader, MAGIC, MAX_NATIONS, NATION_TARGET, WORLD_CHUNKS } from '../client/src/codec.mjs';
import { LOCAL_DIR, namedKey } from './config.mjs';
import { startAndDelegate } from './season.mjs';
import { send } from './send.mjs';
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

/** Offices (over all nations) whose batch for their nation's open tick is in. */
export const countSubmitted = nationHeaders =>
  nationHeaders.reduce((n, x) => n + (x ? x.submitted.filter(t => t === x.openTick).length : 0), 0);

/**
 * Whether the open tick should be resolved now: its input is already
 * frozen, every office submitted, or its deadline (plus a grace second for
 * the ER's clock) passed. Tick 0's deadline was set on base when the
 * government opened and delegation takes a few seconds, so tick 0 gets one
 * full tick on the ER before its deadline counts.
 */
export function isDue({ meta, openTick, submitted, offices, now, delegatedAt = 0, tickSeconds }) {
  if (meta.frozen || submitted >= offices) return true;
  const settling = openTick === 0 && now < delegatedAt + tickSeconds * 1000;
  return !settling && now / 1000 >= meta.deadline + DEADLINE_GRACE_S;
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
    const submitted = countSubmitted(snap.nations);
    const offices = OFFICES * nations;
    if (!isDue({ meta, openTick: open, submitted, offices, now: Date.now(), delegatedAt: this.state.delegatedAt, tickSeconds: this.cfg.tickSeconds })) return;
    // Publish the tick's input on chain first (PS_INPUT chunks; chunk 0
    // freezes it): the program resolves only a published input.
    let published;
    try {
      published = await this.publishInput(open, nations);
    } catch (e) {
      if (chainError(e.message) === 'TooEarly') { this.retryAt = Date.now() + 500; return; } // the ER clock has not reached the deadline yet
      throw e;
    }
    // From each cursor try the furthest stop that worked last time, then
    // shorter ones; every 10 ticks probe further again, since cost depends
    // on the world. Every part is archived as soon as it lands.
    if (open % 10 === 0) this.reach = {};
    const parts = await resolveInParts({
      reach: this.reach,
      resolve: to => send(this.er, this.chain.resolveTick({ nations, to }), [this.crank], `resolve tick ${open} to phase ${to}`),
      onPart: r => appendTickLines(this.tickFile, tickLinesOf(r, published, submitted)),
    });
    this.log(`tick ${open} resolved on ER (${submitted}/${offices} offices submitted, ${parts.map(r => r.cu).join(' + ')} CU)`);
    if (commitDue({ open, commitEvery: this.cfg.commitEvery, commits: this.state.commits })) {
      for (const group of intentGroups({ nations })) {
        await send(this.er, this.chain.commitPart({ payer: this.crank.publicKey, targets: group }), [this.crank], `commit ${group.join(',')}`);
      }
      this.state.commits = (this.state.commits ?? 0) + 1;
      this.store.save();
      this.log(`requested an ER→base commit at tick ${open + 1} (the ER validator settles it on base)`);
    }
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
      await send(this.base, this.chain.finishSeason(), [this.crank], 'finishSeason');
      this.log('season finalized on base: every member\'s payout computed on chain');
    }
    this.phase = 'finalized';
    this.state.finalized = true;
    this.store.save();
  }
}
