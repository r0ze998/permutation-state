// The crank: keeps a season moving. Everything it does is permissionless or
// operator-only liveness; it cannot change outcomes (see permutation-chain
// DESIGN.md, "Trust model").
//
// * Registration: once the expected outside entrants joined (or at once),
//   start the season: genesis, seating, the first election, delegation.
// * Resolve the open tick on the ER once its deadline passed or every
//   office of every nation submitted, and archive the PS_TICK record.
// * Commit the ER state to the base layer every `commitEvery` ticks.
// * After the last tick: commit + undelegate, wait for the world to return
//   to the base layer, and run FinishSeason.
import { appendFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { PublicKey } from '@solana/web3.js';
import { ChainClient, NATION_TARGET } from '../client/src/chain.mjs';
import { chainError, decodeNationHeader, decodeSeason, decodeWorldHeader } from '../client/src/codec.mjs';
import { LOCAL_DIR, namedKey, writeState } from './config.mjs';
import { startAndDelegate } from './season.mjs';
import { send } from './send.mjs';

const hex = b => Buffer.from(b).toString('hex');
const DEADLINE_GRACE_S = 1;
// The ER sponsors at most 10 commits per delegated account (after that it
// asks for a delegated fee payer). One is kept for the final
// commit-and-undelegate, so periodic commits stop at 9.
export const SPONSORED_COMMITS = 10;
const UNDELEGATE_GROUP = 3;
// Phase boundaries a tick may be split at (12 = the whole tick).
const STOPS = [2, 4, 5, 6, 7, 9, 12];

export class Crank {
  constructor({ base, er, cfg, state, log = console.log }) {
    Object.assign(this, { base, er, cfg, state, log });
    this.crank = namedKey('crank');
    this.chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
    this.program = new PublicKey(cfg.programId);
    this.nations = state.nations?.length ?? 6;
    this.snapshot = null; // { world, header, nations: [...headers], slot, at, layer }
    this.busy = false;
    this.phase = state.finalized ? 'finalized' : state.delegated ? 'playing' : 'registering';
    this.openedAt = Date.now();
    mkdirSync(path.join(LOCAL_DIR, 'ticks'), { recursive: true });
    this.tickFile = path.join(LOCAL_DIR, 'ticks', `${state.seasonId}.jsonl`);
  }

  /** Latest world + nation headers from wherever the world lives now. */
  async refresh() {
    const conn = this.phase === 'playing' ? this.er : this.base;
    const chunks = this.chain.worldChunks.length;
    const keys = [...this.chain.worldChunks, ...this.chain.nations(this.nations)];
    const res = await conn.getMultipleAccountsInfoAndContext(keys, 'confirmed');
    const worldParts = res.value.slice(0, chunks);
    const nations = res.value.slice(chunks);
    if (worldParts.some(w => !w)) return null;
    // Reassemble: header + body continue across the chunks in order.
    const world = Buffer.concat(worldParts.map(w => w.data));
    const header = decodeWorldHeader(world);
    if (header.magic !== 'PSWORLD5') return null; // genesis still running
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
    if (!existsSync(this.tickFile)) return [];
    return readFileSync(this.tickFile, 'utf8').split('\n').filter(Boolean).map(l => JSON.parse(l)).filter(r => r.tick >= from);
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
    const season = decodeSeason((await this.base.getAccountInfo(this.chain.season, 'confirmed')).data);
    const hosted = this.state.members.filter(m => m.hosted !== 'external').length;
    const external = season.memberCount - hosted;
    const waited = (Date.now() - this.openedAt) / 1000;
    const enough = external >= this.cfg.waitExternal;
    if (!enough && waited < this.cfg.registrationSeconds) return;
    this.log(`registration closes: ${season.memberCount} members (${external} from outside)`);
    this.state = await startAndDelegate({ base: this.base, er: this.er, cfg: this.cfg, state: this.state, log: this.log });
    this.phase = 'playing';
  }

  async play(snap) {
    const { meta } = snap.header;
    if (meta.finished) {
      this.log('last tick resolved: committing and undelegating');
      const groups = this.intents();
      const done = new Set(this.state.undelegated ?? []);
      for (const group of groups) {
        if (group.every(t => done.has(t))) continue;
        await send(this.er, this.chain.undelegatePart({ payer: this.crank.publicKey, targets: group }), [this.crank], `undelegate ${group.join(',')}`);
        for (const t of group) done.add(t);
        this.state.undelegated = [...done];
        writeState(this.state);
      }
      this.log(`undelegation scheduled for ${groups.flat().length} accounts in ${groups.length} intents`);
      this.phase = 'settling';
      return;
    }
    const open = snap.nations[0]?.openTick;
    const submitted = snap.nations.reduce((n, x) => n + (x ? x.submitted.filter(t => t === x.openTick).length : 0), 0);
    const offices = 4 * this.nations;
    // The program judges the deadline by the ER's clock, which can trail
    // ours by a second: ask a little late instead of being refused.
    // Tick 0's deadline was set on base when the government opened, and
    // delegation takes a few seconds: give it one full tick on the ER.
    const settling = open === 0 && Date.now() < (this.state.delegatedAt ?? 0) + this.cfg.tickSeconds * 1000;
    const due = meta.frozen || (!settling && Date.now() / 1000 >= meta.deadline + DEADLINE_GRACE_S) || submitted === offices;
    if (!due) return;
    // Publish the tick's input on chain first (PS_INPUT chunks; chunk 0
    // freezes it): the program resolves only a published input.
    let input;
    try {
      input = await this.publishInput(open);
    } catch (e) {
      if (chainError(e.message) === 'TooEarly') { this.retryAt = Date.now() + 500; return; } // TooEarly: the ER clock has not reached the deadline yet
      throw e;
    }
    // A tick that does not fit one transaction (compute budget, or the
    // program's heap: its bump allocator never frees) is resolved in parts:
    // `ResolveTick { to }` runs the phases up to `to` and the engine resumes
    // at its phase cursor. From each cursor the crank tries the furthest stop
    // that worked last time, then shorter ones; every 10 ticks it probes
    // further again, since cost depends on the world. Every part is archived
    // as soon as it lands.
    const cus = [];
    if (open % 10 === 0) this.reach = {};
    this.reach ??= {};
    for (let cursor = 0; cursor < 12;) {
      const stops = STOPS.filter(t => t > cursor && t <= (this.reach[cursor] ?? 12)).reverse();
      let done = false;
      for (const to of stops) {
        try {
          const r = await send(this.er, this.chain.resolveTick({ nations: this.nations, to }), [this.crank], `resolve tick ${open} to phase ${to}`);
          this.recordPart(r, submitted, input);
          cus.push(r.cu);
          this.reach[cursor] = to;
          cursor = to;
          done = true;
          break;
        } catch (e) {
          const heavy = /exceeded CUs|ComputationalBudgetExceeded|ProgramFailedToComplete|out of memory/.test(`${e.message} ${(e.logs || []).join(' ')}`);
          if (!heavy || to === stops.at(-1)) throw e;
        }
      }
      if (!done) throw new Error(`tick ${open}: no part from phase ${cursor} fits one transaction`);
    }
    this.log(`tick ${open} resolved on ER (${submitted}/${offices} offices submitted, ${cus.join(' + ')} CU)`);
    const commits = this.state.commits ?? 0;
    if (this.cfg.commitEvery > 0 && (open + 1) % this.cfg.commitEvery === 0 && commits < SPONSORED_COMMITS - 1) {
      for (const group of this.intents()) {
        await send(this.er, this.chain.commitPart({ payer: this.crank.publicKey, targets: group }), [this.crank], `commit ${group.join(',')}`);
      }
      this.state.commits = commits + 1;
      writeState(this.state);
      this.log(`requested an ER→base commit at tick ${open + 1} (the ER validator settles it on base)`);
    }
  }

  /**
   * The season's accounts in small intents, for commits and undelegation.
   * One intent's base-layer finalize runs every target in one transaction,
   * so intents must stay small (measured on devnet): 14 accounts in one
   * intent exceed 64 account keys (magicblock-validator#1693) and Solana's
   * instruction trace, and several densely written world chunks exceed the
   * finalize's compute budget, leaving them stuck undelegating. So: nation
   * accounts in threes, each world chunk alone, chunk 0 (its header says
   * whether the season is over) last.
   */
  intents() {
    const nationTargets = Array.from({ length: this.nations }, (_, c) => NATION_TARGET + c);
    const groups = [];
    for (let i = 0; i < nationTargets.length; i += UNDELEGATE_GROUP) groups.push(nationTargets.slice(i, i + UNDELEGATE_GROUP));
    for (let k = 1; k < this.chain.worldChunks.length; k++) groups.push([k]);
    groups.push([0]);
    return groups;
  }

  /**
   * Log every chunk of the open tick's input (re-logging chunks already
   * published is allowed and harmless) and return the reassembled input,
   * checked against the hash the program logged.
   */
  async publishInput(tick) {
    if (this.published?.tick === tick) return this.published;
    const parts = [];
    let hash = null;
    for (let chunk = 0, total = 1; chunk < total; chunk++) {
      const r = await send(this.er, this.chain.logTickInput({ nations: this.nations, chunk }), [this.crank], `publish tick ${tick} input ${chunk + 1}/${total}`);
      const rec = r.records.find(x => x.tag === 'PS_INPUT');
      if (!rec) throw new Error(`publish ${r.signature}: landed, but its PS_INPUT record could not be read; run scripts/reindex-ticks.mjs`);
      if (rec.tick !== tick || rec.chunk !== chunk || (hash && hex(rec.hash) !== hash)) throw new Error(`publish tick ${tick}: unexpected PS_INPUT (tick ${rec.tick}, chunk ${rec.chunk})`);
      total = rec.total;
      hash = hex(rec.hash);
      parts.push({ bytes: rec.bytes, signature: r.signature });
    }
    const bytes = Buffer.concat(parts.map(p => p.bytes));
    if (createHash('sha256').update(bytes).digest('hex') !== hash) throw new Error(`publish tick ${tick}: the reassembled input does not match its hash`);
    this.published = { tick, input: bytes.toString('hex'), hash, signatures: parts.map(p => p.signature) };
    return this.published;
  }

  /** Archive a resolve transaction's PS_TICK record with its published input; a record that cannot be read stops the crank. */
  recordPart(r, submitted, published) {
    const recs = r.records.filter(x => x.tag === 'PS_TICK');
    if (!recs.length) throw new Error(`resolve ${r.signature}: landed, but its PS_TICK record could not be read (${r.fetched ? 'no record in the logs' : 'transaction not fetchable'}); run scripts/reindex-ticks.mjs`);
    for (const rec of recs) {
      if (hex(rec.inputHash) !== published.hash) throw new Error(`resolve tick ${rec.tick}: the program resolved another input than the one published`);
      const line = { tick: rec.tick, to: rec.to, preRoot: hex(rec.preRoot), root: hex(rec.root), input: published.input, inputHash: published.hash, inputSignatures: published.signatures, signature: r.signature, cu: r.cu, submitted };
      appendFileSync(this.tickFile, JSON.stringify(line) + '\n');
    }
  }

  async settle() {
    // Every chunk and nation account must be back on the base layer.
    const all = await this.base.getMultipleAccountsInfo([...this.chain.worldChunks, ...this.chain.nations(this.nations)], 'confirmed');
    if (all.some(a => !a || !a.owner.equals(this.program))) return; // still undelegating
    const season = decodeSeason((await this.base.getAccountInfo(this.chain.season)).data);
    if (season.status === 'Running') {
      await send(this.base, this.chain.finishSeason(), [this.crank], 'finishSeason');
      this.log('season finalized on base: every member\'s payout computed on chain');
    }
    this.phase = 'finalized';
    this.state.finalized = true;
    writeState(this.state);
  }
}
