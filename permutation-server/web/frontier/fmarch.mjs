// The march: composer model, the send flow and the tracker (contract §5.11,
// §8.3, §9.1, §9.4; web design §7.6–§7.7). Pure functions over decoded
// accounts plus one orchestrated flow (`sendMarch`) whose every step is
// injected, so the node tests drive it without a browser.
//
// Composer: destination (P, Q, tile) → a path from the WASM planner (≤ 32
// steps, ≤ 4 provinces; the program re-checks it at Reveal) → the arrival
// bell in [max(depart + min_lead, earliest), depart + max_lead] → stance →
// retreat ratio from a list with "never" = 0 (I-27) → one of the **three
// tip presets** (tip_min, ⌈1.5 tip_min⌉, 2 tip_min: the relay sponsors no
// other, and there is no zero tip, I-08/I-51) → costs.
//
// Send: seal (worker, self-audit, T(arrive) from the season clock) → the
// marchbook entry saved **before** anything is signed (a march is not sent
// when this browser cannot keep its record) → Depart signed by the session
// key → relay → on chain. A refusal is final (the entry is marked failed
// and may be re-sealed); a lost answer keeps the entry and the tracker
// reconciles it with the Holding's transit records.
import { encodePath, pack, unpack, validate, STANCES, RETREAT_MAX_BPS, MAX_PATH_STEPS, PROVINCE_TILES } from './seal.mjs';
import { minTipLamports, tipPresets, tipPriorityMilli, departEscrow } from '../sdk/frontier/fees.mjs';
import * as book from './marchbook.mjs';
import { tileHex } from './fgeo.mjs';
import { isFinal } from './fland.mjs';

// ------------------------------------------------------------------ rules the composer pre-checks
/**
 * Rules constants the composer and the host panel use to explain a refusal
 * before sending (the program decides). Each is pinned against its Rust
 * definition by web-frontier-march.test.mjs: host.rs `STAMINA_CAP`,
 * travel.rs `MARCH_STAMINA_BASE`/`MARCH_STAMINA_PER_HEX`, the MilliTroops
 * scale (`MILLI`), host.rs `MIN_HOST_TROOPS`.
 */
export const RULES = Object.freeze({ STAMINA_CAP: 120, MARCH_STAMINA_BASE: 10, MARCH_STAMINA_PER_HEX: 2, MILLI: 1000, MIN_HOST_TROOPS: 100 });
/** Depart charges the stamina of a 32-step march whatever the sealed path (I-32): travel::march_stamina(32). */
export const DEPART_STAMINA = RULES.MARCH_STAMINA_BASE + RULES.MARCH_STAMINA_PER_HEX * MAX_PATH_STEPS;
/** Troops of an entry (MilliTroops) as whole troops, rounded down. */
export const troopsOf = milli => Math.floor(Number(milli) / RULES.MILLI);
/** Stamina of an entry at bell b (+1 per bell, capped): host.rs `Stamina::at`. */
export const staminaAt = (entry, b) => (b <= entry.staminaBell ? entry.staminaValue : Math.min(RULES.STAMINA_CAP, entry.staminaValue + (b - entry.staminaBell)));

// ------------------------------------------------------------------ choices
/** The retreat list (§7.6, I-27): never = 0; otherwise withdraw if the frozen defenders exceed ratio × mine. */
export const RETREAT_CHOICES = Object.freeze([
  Object.freeze({ id: 'never', bps: 0 }),
  Object.freeze({ id: 'x2', bps: 20_000 }),
  Object.freeze({ id: 'x1.5', bps: 15_000 }),
  Object.freeze({ id: 'x1', bps: 10_000 }),
  Object.freeze({ id: 'x0.5', bps: 5_000 }),
  Object.freeze({ id: 'custom', bps: null }),
]);

/** retreat_bps of a choice (`custom` takes a ratio such as 1.25), or null when invalid. */
export function retreatBps(choice, ratio) {
  const c = RETREAT_CHOICES.find(x => x.id === choice);
  if (!c) return null;
  if (c.bps !== null) return c.bps;
  const r = Number(ratio);
  if (!Number.isFinite(r)) return null;
  const bps = Math.round(r * 10_000);
  return bps >= 1 && bps <= RETREAT_MAX_BPS ? bps : null;
}

/** The season's minimum tip (lamports, BigInt): §10.1 from the Season's own fields. */
export const seasonTipMin = season => minTipLamports(season.minRevealPriorityMilli, season.revealCuLimit, season.revealLoadedLimit);

/**
 * The three tip presets the composer offers (§9.4): `[{id, lamports,
 * priorityMilli}]` — Standard (tip_min), Decisive (⌈1.5 tip_min⌉), Maximum
 * (2 tip_min). No zero tip and no custom tip: the relay would refuse either.
 */
export function tipOptions(season) {
  const ids = ['standard', 'decisive', 'maximum'];
  return tipPresets(seasonTipMin(season)).map((lamports, i) => ({ id: ids[i], lamports, priorityMilli: tipPriorityMilli(lamports, season.revealCuLimit, season.revealLoadedLimit) }));
}

/** `{tip, marchFee, sealBond, total}` (BigInt lamports; moved from the sponsor into the Holding's escrow at Depart). */
export function marchCosts(season, tip) {
  return { tip: BigInt(tip), marchFee: BigInt(season.marchFee), sealBond: BigInt(season.sealBond), total: departEscrow({ MARCH_FEE: season.marchFee, SEAL_BOND: season.sealBond }, tip) };
}

/** The arrival bells a march departing at `departBell` may name: `{min, max}` (min also ≥ the planner's earliest bell). */
export function arrivalWindow(season, departBell, earliest = null) {
  const min = Math.max(departBell + Number(season.minLead), earliest ?? 0);
  return { min, max: departBell + Number(season.maxLead) };
}

// ------------------------------------------------------------------ the holding's transits
/** The first free transit record (state 0), or null. */
export const freeTransitSlot = holding => { const i = holding.transit.findIndex(t => t.state === 0); return i < 0 ? null : i; };
/** Whether a host appears in a transit record in state 1–3 (I-44: it cannot Depart, Dissolve or Explore until settled). */
export const hostInTransit = (holding, hostId) => holding.transit.some(t => t.state >= 1 && t.state <= 3 && BigInt(t.hostId) === BigInt(hostId));

// ------------------------------------------------------------------ the path (WASM planner)
/**
 * Ask the kernel for a path from the host's tile to the destination:
 * `{ok, route: {dirs, pathLen, path, secs, hexes, provinces}}` or `{ok: false, code}`.
 * `seeds`: the season's ring seeds `[{ring, seed (hex)}]` (/h/season `rings`).
 * The secs are the unit's pace without the doctrine's travel bias (every
 * M1 doctrine's is ≤ 1.0). The earliest bell also depends on when Depart
 * lands: the page computes it for a departure DEPART_MARGIN_SECS later and
 * recomputes it at send time (sendMarch `earliestAtSend`).
 */
export function planRoute(kernel, { from, to, unit, seeds, blocked = [] }) {
  if (!kernel) return { ok: false, code: 'NoKernel' };
  const start = tileHex(from.p, from.q, from.tile), dest = tileHex(to.p, to.q, to.tile);
  if (!start || !dest) return { ok: false, code: 'BadTile' };
  const r = kernel.call('plan_path', { start, dest, unit, blocked, seeds });
  if (!r.ok) return { ok: false, code: 'PlannerRefused', reason: r.reason ?? null };
  if (!r.value) return { ok: false, code: 'NoPath' };
  if (r.value.provinces.length > 4 || r.value.dirs.length > MAX_PATH_STEPS) return { ok: false, code: 'Path' };
  return { ok: true, route: r.value };
}

/**
 * Seconds the page adds to "now" when it computes the earliest arrival bell
 * (wave-3 review, W3-F): the program uses the Depart's own landing time, so
 * an earliest bell computed at compose time goes stale if the send is
 * delayed or lands a few seconds later across a bell boundary. The composer
 * and the send both assume a departure this much later.
 */
export const DEPART_MARGIN_SECS = 90;

/** The earliest arrival bell of a route departing at chain time `departTs` (kernel), or null without a kernel. */
export function earliestBell(kernel, { genesisTs, departTs, secs }) {
  if (!kernel) return null;
  const r = kernel.call('earliest_arrival_bell', { genesis_ts: genesisTs, depart_ts: Math.floor(departTs), secs });
  return r.ok ? r.value : null;
}

// ------------------------------------------------------------------ the check before sealing
/**
 * Everything the composer can check before it seals (the program checks it
 * all again): `m = {season, nowBell, resolvedNext, holding, host (Entry),
 * dest {p, q, tile}, route, earliest, arriveBell, stance, retreat: {choice,
 * ratio}, tip}` → `[code, …]` (empty = ready to seal). Codes are program
 * error names where the program has one.
 */
export function checkMarch(m) {
  const out = [];
  const add = c => { if (!out.includes(c)) out.push(c); };
  if (!m.holding || !isFinal(m.holding, m.province ?? null, m.now ?? 0, m.nowBell)) add('NotFinal');
  if (m.resolvedNext === null || m.resolvedNext === undefined || m.resolvedNext + 1 < m.nowBell) add('NotResident');
  const h = m.host;
  if (!h || h.state !== 1) add('HostBusy');
  else {
    if (h.pendOp !== 0) add('HostBusy');
    if (m.holding && hostInTransit(m.holding, h.id)) add('HostInTransit');
    if (m.nowBell < h.readyBell || staminaAt(h, m.nowBell) < DEPART_STAMINA) add('Cooldown');
  }
  if (m.holding && freeTransitSlot(m.holding) === null) add('TransitState');
  if (!m.dest || !(m.dest.tile >= 0 && m.dest.tile < PROVINCE_TILES)) add('NoDestination');
  if (!m.route) add('NoPath');
  else if (m.route.dirs.length < 1 || m.route.dirs.length > MAX_PATH_STEPS || m.route.provinces.length > 4) add('Path');
  const w = arrivalWindow(m.season, m.nowBell, m.earliest);
  if (!Number.isInteger(m.arriveBell) || m.arriveBell < w.min || m.arriveBell > w.max) add('ArrivalBell');
  if (!(m.stance >= 0 && m.stance < STANCES.length)) add('Stance');
  if (retreatBps(m.retreat?.choice, m.retreat?.ratio) === null) add('Retreat');
  const presets = tipOptions(m.season).map(o => o.lamports);
  let tip = null;
  try { tip = BigInt(m.tip); } catch { tip = null; }
  if (tip === null || tip === 0n || tip < presets[0]) add('TipTooLow');
  else if (!presets.includes(tip)) add('TipNotPreset');
  return out;
}

/** The 37-byte plaintext of a checked march (validated as the program will). Throws on an invalid one. */
export function plaintextOf(m) {
  const p = encodePath(m.route.dirs);
  if (!p) throw new Error('path');
  const plain = pack({ version: 1, hostId: BigInt(m.host.id), arriveBell: m.arriveBell, destP: m.dest.p, destQ: m.dest.q, destTile: m.dest.tile,
    stance: m.stance, retreatBps: retreatBps(m.retreat.choice, m.retreat.ratio), pathLen: p.pathLen, path: p.path });
  const why = validate(unpack(plain), BigInt(m.host.id), m.arriveBell);
  if (why) throw new Error(`plaintext: ${why}`);
  return plain;
}

// ------------------------------------------------------------------ the send flow
export const SEND_STEPS = Object.freeze(['sealing', 'saved', 'sent', 'onChain']);

/**
 * Seal, save, sign, send, confirm. `m` as checkMarch plus `holdingAddress`,
 * `holdingKey {p, q, site}`; `deps`:
 *   seal(req) → sealMarch result; clock; publicKey (the season's beacon key);
 *   storage, bookKey; submit(req) → fplay.submit; track(sig) → fplay.track;
 *   accounts (fplay.accountsFor('Depart', …)); session (the signing key);
 *   onStep(step, detail); earliestAtSend() → the earliest arrival bell for a
 *   departure now + DEPART_MARGIN_SECS (null: unknown), checked again here
 * → `{ok, step, code?, signature?, entry}`. Never rejects.
 */
export async function sendMarch(m, deps) {
  const step = (s, d) => { try { deps.onStep?.(s, d); } catch { /* the UI's problem */ } };
  // The earliest bell again, for a departure at "now + margin" (the compose-time one may be stale).
  let fresh = null;
  try { fresh = (await deps.earliestAtSend?.()) ?? null; } catch { fresh = null; }
  if (fresh !== null && fresh > (m.earliest ?? 0)) m = { ...m, earliest: fresh };
  const problems = checkMarch(m);
  if (problems.length) return { ok: false, step: 'check', code: problems[0], problems, earliest: m.earliest ?? null };
  let plain;
  try { plain = plaintextOf(m); } catch (e) { return { ok: false, step: 'check', code: 'BadPlaintext', error: e.message }; }
  const transitSlot = freeTransitSlot(m.holding);
  step('sealing');
  let s;
  try {
    s = await deps.seal({ plain, clock: deps.clock, publicKey: deps.publicKey, hostId: BigInt(m.host.id), arriveBell: m.arriveBell });
  } catch (e) {
    s = { ok: false, code: e?.code ?? 'SealFailed', error: String(e?.message ?? e) };
  }
  if (!s?.ok) return { ok: false, step: 'sealing', code: s?.code ?? 'SealFailed', error: s?.error };
  // The record first: a lost answer must never lose a sealed order (§9.1).
  let entry = book.entryOf({ host: BigInt(m.host.id), transitSlot, departBell: m.nowBell, arriveBell: m.arriveBell, holding: m.holdingAddress,
    plain, salt: s.salt, commit: s.commit, sealRoot: s.sealRoot, ctHash: s.ctHash, seal: s.seal, round: s.round, tip: BigInt(m.tip),
    route: m.route ?? null, origin: m.province && Number.isInteger(m.host?.tile) ? { p: m.province.p, q: m.province.q, tile: m.host.tile } : null });
  let current;
  try { current = book.addSealed(book.loadBook(deps.storage, deps.bookKey), entry); } catch (e) { return { ok: false, step: 'saved', code: 'AlreadySent', error: e.message }; }
  if (!book.saveBook(deps.storage, deps.bookKey, current)) return { ok: false, step: 'saved', code: 'NotSaved' };
  step('saved', entry);
  const save = patch => {
    current = book.mark(book.loadBook(deps.storage, deps.bookKey), entry, patch);
    book.saveBook(deps.storage, deps.bookKey, current);
    entry = current.find(e => e.host === entry.host && e.transitSlot === entry.transitSlot && e.arriveBell === entry.arriveBell) ?? entry;
  };
  const r = await deps.submit({ name: 'Depart', accounts: deps.accounts, signer: { session: deps.session },
    fields: { host_id: BigInt(m.host.id), commit: s.commit, seal: s.seal, arrive_bell: m.arriveBell, tip: BigInt(m.tip), transit_slot: transitSlot } });
  if (!r.ok) {
    // Refused before sending (or by the relay): nothing is on chain; the entry may be re-sealed.
    // A lost answer ('network') may still land: the entry stays 'sealed' and reconciles later.
    if (r.code !== 'network') save({ state: 'failed', failure: r.code });
    return { ok: false, step: 'sent', code: r.code, error: r.error, entry };
  }
  save({ state: 'sent', signature: r.signature });
  step('sent', r);
  const t = await deps.track(r.signature);
  if (!t.ok) {
    if (t.state === 'failed' || t.state === 'expired') save({ state: 'failed', failure: t.code });
    return { ok: false, step: 'onChain', code: t.code, programCode: t.programCode, signature: r.signature, entry };
  }
  save({ state: 'landed' });
  step('onChain', t);
  return { ok: true, step: 'onChain', signature: r.signature, slot: t.slot, entry };
}

// ------------------------------------------------------------------ the tracker
export const TRACKER_STEPS = Object.freeze(['departed', 'arrivalBell', 'anchored', 'revealed', 'resolved', 'settled']);

/**
 * The tracker of one march (§7.6): `{steps: [{id, state: 'done'|'now'|'next'}],
 * locked, route, outcome}`.
 *   entry   the marchbook entry, or null (another device: keepers reveal)
 *   transit the Holding's transit record for it, or null (free/unknown)
 *   nowBell, pipeline (clock.pipeline state of the arrival bell in its
 *   region: 'open'…'resolved'), slotPresent, settled {outcome} (TRANSIT_SETTLED)
 */
export function tracker({ entry = null, transit = null, nowBell, pipeline = null, slotPresent = false, settled = null }) {
  const arrive = transit?.arriveBell ?? entry?.arriveBell ?? null;
  const live = !!transit && transit.state >= 1 && transit.state <= 3;
  const done = {
    departed: live || !!settled || ['landed', 'revealing', 'revealed', 'settled'].includes(entry?.state),
    arrivalBell: arrive !== null && nowBell >= arrive,
    anchored: ['revealing', 'awaitingSeed', 'resolving', 'resolved'].includes(pipeline),
    revealed: slotPresent || entry?.state === 'revealed' || (!!settled && !['Routed', 'BouncedUnranked', 'BadSeal'].includes(settled.outcome)),
    resolved: pipeline === 'resolved' || !!settled,
    settled: !!settled || entry?.state === 'settled',
  };
  let seenNow = false;
  const steps = TRACKER_STEPS.map(id => {
    if (done[id]) return { id, state: 'done' };
    if (!seenNow) { seenNow = true; return { id, state: 'now' }; }
    return { id, state: 'next' };
  });
  // After the close without a slot the march is routed (or was refused a slot) unless a keeper revealed it.
  const closedUnrevealed = ['awaitingSeed', 'resolving', 'resolved'].includes(pipeline) && !done.revealed;
  const route = !entry ? 'keepers' : entry.state === 'revealing' || entry.attempts?.some(a => a.route === 'self') ? 'self' : 'keepers';
  return { steps, locked: live, route, arriveBell: arrive, closedUnrevealed, outcome: settled?.outcome ?? null, hint: book.trackerHint(entry) };
}

// ------------------------------------------------------------------ incoming arrivals (§7.7)
/**
 * Warnings for the viewer's holdings from public departures: for each
 * departure of another faction whose origin could reach one of the
 * holdings by its arrival bell (kernel `reachable`, optimistic: a "no" is
 * certain, a "yes" is only a warning), one line per (holding, bell): `{holding
 * {p, q, site}, bell, hosts, troops}`. `departures`: DEPART records
 * `{host_id, origin_p, origin_q, origin_tile, depart_bell, arrive_bell, dep_mass,
 * faction}`; `holdings`: `[{p, q, site, tile}]`. Without a kernel: null
 * (the page says the warnings need the rules module).
 */
export function incomingWarnings({ kernel, departures = [], holdings = [], faction, genesisTs, nowBell, unitOf = () => 0 }) {
  if (!kernel) return null;
  const out = new Map();
  for (const d of departures) {
    if (d.faction === faction || d.arrive_bell < nowBell) continue;
    const origin = tileHex(d.origin_p, d.origin_q, d.origin_tile);
    for (const h of holdings) {
      const dest = tileHex(h.p, h.q, h.tile);
      if (!origin || !dest) continue;
      const r = kernel.call('reachable', { origin, dest, genesis_ts: genesisTs, depart_ts: genesisTs + 600 * d.depart_bell, target_bell: d.arrive_bell, unit: unitOf(d) });
      if (!r.ok || !r.value) continue;
      const key = `${h.p},${h.q},${h.site}@${d.arrive_bell}`;
      const w = out.get(key) ?? { holding: { p: h.p, q: h.q, site: h.site }, bell: d.arrive_bell, hosts: 0, troops: 0 };
      w.hosts += 1;
      w.troops += troopsOf(d.dep_mass);
      out.set(key, w);
    }
  }
  return [...out.values()].sort((a, b) => a.bell - b.bell);
}
