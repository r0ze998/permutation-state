// The crank: keeps a season moving. Everything it does is permissionless or
// operator-only liveness; it cannot change outcomes (see permutation-chain
// DESIGN.md, "Trust model").
//
// * Resolve the open tick on the ER once its deadline passed or every civ
//   submitted, and archive the PS_TICK record from the logs.
// * Commit the ER state to the base layer every `commitEvery` ticks.
// * After the last tick: commit + undelegate, wait for the world to return to
//   the base layer, and run FinishSeason.
import { appendFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { PublicKey } from '@solana/web3.js';
import { ChainClient, ORDERS_TARGET } from '../client/src/chain.mjs';
import { decodeOrdersHeader, decodeSeason, decodeWorldHeader } from '../client/src/codec.mjs';
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

export class Crank {
  constructor({ base, er, cfg, state, log = console.log }) {
    Object.assign(this, { base, er, cfg, state, log });
    this.crank = namedKey('crank');
    this.chain = new ChainClient(cfg.programId, BigInt(state.seasonId));
    this.program = new PublicKey(cfg.programId);
    this.civs = state.civs.length;
    this.snapshot = null; // { world: Buffer, orders: [...headers], slot, at }
    this.busy = false;
    this.phase = state.finalized ? 'finalized' : state.delegated ? 'playing' : 'setup';
    mkdirSync(path.join(LOCAL_DIR, 'ticks'), { recursive: true });
    this.tickFile = path.join(LOCAL_DIR, 'ticks', `${state.seasonId}.jsonl`);
  }

  /** Latest world + orders headers from wherever the world lives now. */
  async refresh() {
    const conn = this.phase === 'playing' ? this.er : this.base;
    const chunks = this.chain.worldChunks.length;
    const keys = [...this.chain.worldChunks, ...this.chain.ordersList(this.civs)];
    const res = await conn.getMultipleAccountsInfoAndContext(keys, 'confirmed');
    const worldParts = res.value.slice(0, chunks);
    const orders = res.value.slice(chunks);
    if (worldParts.some(w => !w)) return null;
    // Reassemble: header + body continue across the chunks in order.
    const world = { data: Buffer.concat(worldParts.map(w => w.data)) };
    this.snapshot = {
      world: world.data,
      header: decodeWorldHeader(world.data),
      orders: orders.map(o => (o ? decodeOrdersHeader(o.data) : null)),
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
      if (this.phase === 'setup') return await this.setup();
      const snap = await this.refresh();
      if (!snap) return;
      if (this.phase === 'playing') await this.play(snap);
      else if (this.phase === 'settling') await this.settle();
    } catch (e) {
      this.log(`crank: ${e.message}${e.logs?.length ? `\n  ${e.logs.slice(-4).join('\n  ')}` : ''}`);
      this.retryAt = Date.now() + 3000; // back off instead of hammering a failing step
    } finally {
      this.busy = false;
    }
  }

  /** Entry open: once every seat is taken (hosted or via x402), start the season. */
  async setup() {
    const season = decodeSeason((await this.base.getAccountInfo(this.chain.season, 'confirmed')).data);
    for (const [i, c] of season.civs.entries()) {
      if (!this.state.civs.some(x => x.civ === i)) {
        this.state.civs.push({ civ: i, name: c.name, kind: c.kind, hosted: 'external', wallet: new PublicKey(c.player).toBase58(), session: new PublicKey(c.session).toBase58() });
        this.log(`civ ${i} ${c.name} joined from outside (x402)`);
      }
    }
    this.civs = season.civs.length;
    if (season.civs.length < season.maxCivs) return;
    this.log('all seats taken: starting the season');
    this.state = await startAndDelegate({ base: this.base, er: this.er, cfg: this.cfg, state: this.state, log: this.log });
    this.phase = 'playing';
  }

  async play(snap) {
    const { meta } = snap.header;
    if (meta.finished) {
      this.log('last tick resolved: committing and undelegating');
      // One intent's base-layer finalize runs all its undelegations in one
      // transaction; 14 exceed Solana's instruction-trace limit. Small groups,
      // world chunk 0 (it carries the "finished" header) last.
      const targets = [...Array.from({ length: this.civs }, (_, c) => ORDERS_TARGET + c), ...this.chain.worldChunks.map((_, k) => k).filter(k => k !== 0), 0];
      const done = new Set(this.state.undelegated ?? []);
      for (let i = 0; i < targets.length; i += UNDELEGATE_GROUP) {
        const group = targets.slice(i, i + UNDELEGATE_GROUP);
        if (group.every(t => done.has(t))) continue;
        await send(this.er, this.chain.undelegatePart({ payer: this.crank.publicKey, targets: group }), [this.crank], `undelegate ${group.join(',')}`);
        for (const t of group) done.add(t);
        this.state.undelegated = [...done];
        writeState(this.state);
      }
      this.log(`undelegation scheduled for ${targets.length} accounts in ${Math.ceil(targets.length / UNDELEGATE_GROUP)} groups`);
      this.phase = 'settling';
      return;
    }
    const open = snap.orders[0]?.openTick;
    const submitted = snap.orders.filter(o => o?.hasBatch && o.batchTick === o.openTick).length;
    // The program judges the deadline by the ER's clock, which can trail
    // ours by a second: ask a little late instead of being refused.
    const due = Date.now() / 1000 >= meta.deadline + DEADLINE_GRACE_S || submitted === this.civs;
    if (!due) return;
    let r;
    try {
      r = await send(this.er, this.chain.resolveTick({ civs: this.civs }), [this.crank], `resolve tick ${open}`);
    } catch (e) {
      if (/"Custom":16\b/.test(e.message)) { this.retryAt = Date.now() + 500; return; } // TooEarly: the ER clock has not reached the deadline yet
      throw e;
    }
    for (const rec of r.records.filter(x => x.tag === 'PS_TICK')) {
      const line = { tick: rec.tick, to: rec.to, vrf: hex(rec.vrf), preRoot: hex(rec.preRoot), root: hex(rec.root), batches: hex(rec.batches), signature: r.signature, cu: r.cu, submitted };
      appendFileSync(this.tickFile, JSON.stringify(line) + '\n');
    }
    this.log(`tick ${open} resolved on ER (${submitted}/${this.civs} submitted, ${r.cu} CU)`);
    const commits = this.state.commits ?? 0;
    if ((open + 1) % this.cfg.commitEvery === 0 && commits < SPONSORED_COMMITS - 1) {
      await send(this.er, this.chain.commit({ payer: this.crank.publicKey, civs: this.civs }), [this.crank], 'commit');
      this.state.commits = commits + 1;
      writeState(this.state);
      this.log(`requested an ER→base commit at tick ${open + 1} (the ER validator settles it on base)`);
    }
  }

  async settle() {
    // Every chunk and orders account must be back on the base layer.
    const all = await this.base.getMultipleAccountsInfo([...this.chain.worldChunks, ...this.chain.ordersList(this.civs)], 'confirmed');
    if (all.some(a => !a || !a.owner.equals(this.program))) return; // still undelegating
    const season = decodeSeason((await this.base.getAccountInfo(this.chain.season)).data);
    if (season.status === 'Running') {
      await send(this.base, this.chain.finishSeason(), [this.crank], 'finishSeason');
      this.log('season finalized on base: payouts computed on chain');
    }
    this.phase = 'finalized';
    this.state.finalized = true;
    writeState(this.state);
  }
}
