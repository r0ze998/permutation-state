// Reading the season: health, the season account and member registry, the
// history layer, the open tick's phase, the raw world, and the tick index
// for the replay verifier. (The AI roster and members' messages:
// roster.mjs.)
//
//   GET /health
//   GET /season        season account, members, records for the verifier, and
//                      registration: {members, aiCount, openedAt, closesAt, serverNow, entryFee,
//                      deposit, open} (times in ms since the epoch; closesAt null in dev mode);
//                      devWallet (true only on localnet with --dev-wallet). With operator AI
//                      members nobody's self-declared kind is shown (members[] has no
//                      kind/attested, V5 §18.2). `endpoints` {base, er}: the RPC URLs, on the
//                      operator listener; on the public one only those given with
//                      --public-base-rpc / --public-er-rpc (none: no `endpoints`)
//   GET /history       the season it follows, its PS_HISTORY record once finalized, the lineage
//   GET /ticks?from=N  archived tick records: PS_TICK roots with the input its PS_INPUT
//                      chunks published (an index for the replay verifier, which re-reads
//                      both from the ER's logs)
//   GET /tick          the open tick's sealed-orders phase (commit | reveal | frozen | finished),
//                      and per nation which offices committed and revealed for it
//   GET /world.bin     raw world account (octet-stream) from the layer it lives on
import { PublicKey } from '@solana/web3.js';
import { NATIONS } from '../../client/src/codec.mjs';
import { RouteError, routeSeason } from './errors.mjs';
import { toHex as hex } from '../../client/src/bytes.mjs';
import { countSeals } from '../crank.mjs';
import { registrationOf, registrationOpen } from '../season.mjs';

const b58 = k => new PublicKey(k).toBase58();

/** A registry member as /season shows it in a season with operator AI members: without the self-declared kind. */
export const publicMember = ({ kind, attested, ...m }) => m;

/**
 * Per nation, which offices committed a batch for its open tick and which
 * of those were revealed (public header data, the same for every office):
 * `[{civ, committed: [4 × bool], revealed: [4 × bool]}]`.
 */
export const officeSeals = nationHeaders => nationHeaders.map((h, civ) => (h
  ? { civ, committed: h.committed.map(t => t === h.openTick), revealed: h.submitted.map(t => t === h.openTick) }
  : { civ, committed: [false, false, false, false], revealed: [false, false, false, false] }));
/**
 * The RPC URLs a listener shows: the operator's own on the operator
 * listener; on the public one only the ones the operator chose to publish
 * (its own may carry an API key). Null when there is none to show.
 */
export function endpointsFor(cfg, surface = 'operator') {
  if (surface !== 'public') return { base: cfg.baseRpc, er: cfg.erRpc };
  const out = { ...(cfg.publicBaseRpc ? { base: cfg.publicBaseRpc } : {}), ...(cfg.publicErRpc ? { er: cfg.publicErRpc } : {}) };
  return Object.keys(out).length ? out : null;
}

/** How old a world snapshot /world.bin may serve (ms). */
const SNAPSHOT_MAX_AGE_MS = 300;

export const seasonRoutes = {
  'GET /health': async ({ crank, store }) => ({ body: { ok: true, phase: crank.phase, season: store.state.seasonId } }),

  'GET /season': async ({ base, chain, cfg, crank, store, registry, now }, req) => {
    const s = await routeSeason({ base, chain });
    const state = store.state;
    const t = now();
    const reg = registrationOf(state, { fallbackOpenedAt: crank.startedAt ?? t, entryFee: s.entryFee });
    // With operator AI members nobody's self-declared kind is shown (it is
    // 2, undeclared, for everyone who joined through this gateway anyway).
    const members = (await registry.list()).map(m => (s.aiCount > 0 ? publicMember(m) : m));
    const endpoints = endpointsFor(cfg, req?.surface);
    // What a page served to the public may show (the play server relays
    // /season from the operator listener): only the published RPC URLs.
    const publicEndpoints = endpointsFor(cfg, 'public');
    return {
      body: {
        season: { ...s, admin: b58(s.admin), crank: b58(s.crank), usdcMint: b58(s.usdcMint) },
        nations: NATIONS.slice(0, s.nations), members,
        programId: cfg.programId, cluster: cfg.cluster, phase: crank.phase,
        registration: { members: s.memberCount, aiCount: s.aiCount, openedAt: reg.openedAt, closesAt: reg.closesAt, serverNow: t,
          entryFee: s.entryFee.toString(), deposit: reg.deposit.toString(), open: registrationOpen({ state, season: s, phase: crank.phase, now: t }) },
        devWallet: !!cfg.devWallet && cfg.cluster === 'localnet',
        accounts: { season: chain.season.toBase58(), world: chain.world.toBase58(), vault: chain.vault.toBase58(), nations: chain.nations(s.nations).map(k => k.toBase58()) },
        genesis: state.genesis ?? null, seating: state.seating ?? [], open: state.open ?? null, ...(endpoints ? { endpoints } : {}), publicEndpoints,
      },
    };
  },

  // The history layer: the season it follows and, once finalized, its
  // record (PS_HISTORY) and history root, which the next season takes over.
  'GET /history': async ({ base, chain, store }) => {
    const s = await routeSeason({ base, chain });
    return { body: { season: String(s.seasonId), prevSeasonId: String(s.prevSeasonId), prevHistoryRoot: hex(s.prevHistoryRoot),
      historyRoot: s.status === 'Finalized' ? hex(s.historyRoot) : null, record: store.state.history?.record ?? null, signature: store.state.history?.signature ?? null,
      // The records of the seasons before, oldest first (see season.mjs).
      lineage: store.state.lineage ?? [] } };
  },

  'GET /ticks': async ({ crank }, req) => ({ body: { records: crank.tickRecords(Number(req.url.searchParams.get('from') || 0)) } }),

  // The open tick's sealed-orders phase: `commit` (CommitOrders until
  // `deadline`), `reveal` (RevealOrders until `deadline`), `frozen` (being
  // published and resolved). Agents reveal when it says `reveal`.
  'GET /tick': async ({ crank, now }) => {
    const snap = now() - (crank.snapshot?.at ?? 0) < SNAPSHOT_MAX_AGE_MS ? crank.snapshot : await crank.refresh();
    if (!snap) throw new RouteError(503, 'world not available', 'WorldUnavailable');
    const { meta } = snap.header;
    const { committed, revealed } = countSeals(snap.nations);
    const phase = meta.finished ? 'finished' : meta.frozen ? 'frozen' : meta.revealing ? 'reveal' : 'commit';
    return { body: { tick: snap.nations[0]?.openTick ?? null, phase, deadline: meta.deadline, committed, revealed, slot: snap.slot, nations: officeSeals(snap.nations) } };
  },

  'GET /world.bin': async ({ crank, now }) => {
    const snap = now() - (crank.snapshot?.at ?? 0) < SNAPSHOT_MAX_AGE_MS ? crank.snapshot : await crank.refresh();
    if (!snap) throw new RouteError(503, 'world not available', 'WorldUnavailable');
    return {
      raw: snap.world,
      headers: { 'Content-Type': 'application/octet-stream', 'X-Slot': String(snap.slot), 'X-Layer': snap.layer, 'X-Phase': crank.phase },
    };
  },
};
