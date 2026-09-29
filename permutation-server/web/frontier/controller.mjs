// The play controller (W3-F): what the game page does between the herald
// (reads), the relay (writes) and the screens. It keeps the viewer's state
// in FS (fstate.mjs), refreshes it on the herald poll schedule (§4.2: 10 s
// while a march or a transaction is in flight, else 30 s; paused while the
// page is hidden, except the marchbook's self-reveal timer), runs the
// self-reveal (I-24, §9.1), and turns every `data-act` of the screens into
// one action: build → check → sign → relay → track (fplay.mjs), the march
// through fmarch.sendMarch. Nothing here decides a rule; the program does.
import { FS, invalidate } from './fstate.mjs';
import * as io from './fchainio.mjs';
import * as fsession from './fsession.mjs';
import * as book from './marchbook.mjs';
import { submit, track, accountsFor, campMaskOf, campWinner } from './fplay.mjs';
import { sendMarch, planRoute, earliestBell, arrivalWindow, tipOptions, incomingWarnings, DEPART_MARGIN_SECS } from './fmarch.mjs';
import { landState, hostsIn } from './fland.mjs';
import { toggleTile } from './screens/explore.mjs';
import { sealMarch, unpack, ctHash as ctHashOf, sealRoot as sealRootOf } from './seal.mjs';
import { bellAt, bellStart, pipeline, archivePartOf } from './clock.mjs';
import { regionOf } from './fgeo.mjs';
import { hostParts } from './faddr.mjs';
import { decodePage } from './flog.mjs';
import { nextPoll } from './herald.mjs';
import { kernel as loadKernel, UNITS } from './wasm.mjs';
import { uiKey, uiStorage, loadUi, saveUi } from './fui.mjs';
import * as wallet from '../wallet.mjs';
import { decode as fromBase58, encode as toBase58 } from '../sdk/base58.mjs';
import { fromBase64 } from '../sdk/bytes.mjs';
import { L } from '../lang.mjs';

const SESSION_DAYS = 30 * 86_400;
const now = () => FS.chain?.now() ?? 0;
const nowBell = () => (FS.clock ? bellAt(FS.clock.genesisTs, now()) ?? 0 : 0);
const scope = () => io.scope();
/** Save UI preferences once a season is pinned (convenience only). */
const saveUiPatch = patch => { const sc = scope(); if (sc) FS.ui = saveUi(uiStorage, uiKey(sc), patch); };
const bookKey = () => (FS.wallet && scope() ? book.bookKey(scope(), FS.wallet.address) : null);
// Notices keep a thunk or the failure itself, so a language switch re-renders them in the new language.
const setNotice = n => { FS.notice = n; invalidate('panel'); };
const busy = text => setNotice({ busy: true, text });
const done = text => setNotice({ ok: true, text });
const failed = r => {
  if (r?.code === 'InviteRequired') FS.inviteRequired = true;
  return setNotice({ ok: false, ...r, text: r?.text ?? null });
};

/** The addresses and signer context of a player action. */
function ctx(extra = {}) {
  const pin = io.pinned();
  return { addresses: pin.addresses, wallet: FS.wallet.address, actor: FS.session?.publicKey, faction: FS.citizen?.faction, ...extra };
}

/** One sponsored action, tracked to its landing; the notice says how it went. */
async function act(name, { v = {}, fields = {}, signer, requester = null, label, after } = {}) {
  // `signer: null` = a settle shape (no authority signs; the session key signs the request only).
  if (!io.pinned()) return failed({ code: 'NoPin' });
  if (!FS.wallet) return failed({ code: 'NoWallet' });
  let sign = null;
  if (signer !== null) {
    if (name === 'Join' || name === 'SetSession') {
      const w = wallet.connected();
      if (!w) return failed({ code: 'NoWallet' });
      sign = { wallet: w };
    } else {
      if (!FS.session) return failed({ code: 'NoSession' });
      sign = { session: FS.session };
    }
  }
  busy(label ?? (() => L`送信中…`));
  FS.inFlight = (FS.inFlight ?? 0) + 1;
  try {
    const accounts = typeof v === 'function' ? fp => accountsFor(name, v(fp)) : accountsFor(name, ctx(v));
    const send = () => submit({ name, accounts, fields, signer: sign, requester, citizen: requester ? io.pinned().addresses.of('Citizen', { wallet: FS.wallet.address }) : undefined, invite: v.invite });
    let r = await send();
    let t = r.ok ? await track(r.signature) : r;
    // W6-D: a resident action refused because the province fell a bell behind between the check and the landing:
    // ask the keeper to catch it up, wait for the herald to show it, and send once more.
    if (!t.ok && retryAfterCatchUp(name, t.code)) {
      busy(() => L`州が追いつくのを待っています…`);
      if (await catchUp()) {
        r = await send();
        t = r.ok ? await track(r.signature) : r;
      }
    }
    if (!r.ok) return failed(r);
    if (!t.ok) return failed(t);
    done(() => L`チェーンに記録されました`);
    await after?.(r);
    return r;
  } finally {
    FS.inFlight -= 1;
    refreshSoon();
  }
}

/** Ask the keeper to catch the home province up and wait (≤ 20 s) until the herald shows it resolved through the bell before this one. */
async function catchUp() {
  const h = FS.holdings?.[0];
  if (!h) return false;
  const r = await io.nudge(h.p, h.q, FS.provinces.get(`${h.p},${h.q}`)?.province.resolvedNext ?? nowBell());
  if (!r.ok) return false;
  FS.autoNudgeBell = nowBell();
  for (let i = 0; i < 10; i++) {
    await new Promise(res => setTimeout(res, 2000));
    await loadEnvelope(h.p, h.q);
    const pr = FS.provinces.get(`${h.p},${h.q}`)?.province;
    if (pr && !provinceLags(pr, nowBell())) return true;
  }
  return false;
}

// ------------------------------------------------------------------ pure rules of the controller (tested)
/** Resident actions sent again once after a catch-up when the program answers NotResident (not Depart: the march has its own path). */
export const CATCH_UP_RETRY = Object.freeze(['Muster', 'Garrison', 'Dissolve', 'Explore']);
export const retryAfterCatchUp = (name, code) => code === 'NotResident' && CATCH_UP_RETRY.includes(name);
/** The program's resident rule: a province lags when it is not resolved through `nowBell − 2` (`resolved_next + 1 < b`). */
export const provinceLags = (province, nowBell) => !province || province.resolvedNext + 1 < nowBell;
/**
 * Whether the page asks the keeper to catch the viewer's home province up
 * on its own (W6-D): the keeper skips quiet provinces in batches (up to 24
 * bells), so a player's province usually lags and every resident action
 * waited for a tap on "catch up". Once per bell, only while the page is
 * visible and the viewer holds land there.
 */
export function shouldAutoNudge({ holding, province, nowBell, lastBell = null, hidden = false }) {
  if (hidden || !holding || !(holding.state === 1 || holding.state === 2) || !province) return false;
  if (!provinceLags(province, nowBell)) return false;
  return lastBell !== nowBell;
}

/** Events the chronicle starts behind the head (a fresh page starts near the head, not at event 1). */
export const CHRONICLE_WINDOW = 500;
/** Pages of /h/events read per refresh while the answer is `full`. */
export const CHRONICLE_PAGES = 4;
/** The chronicle's first cursor for the season record's `headSeq` (wave-3 review, W3-F). */
export const chronicleStart = headSeq => Math.max(0, Number(headSeq ?? 0) - CHRONICLE_WINDOW);
/**
 * The unit the incoming warnings judge a departure with: the fastest pace
 * (cavalry), so `reachable` stays optimistic — a "no" is certain — whatever
 * unit the host is (DEPART does not log it).
 */
export const WARNING_UNIT = UNITS.indexOf('Horseman');
/** Seconds between polls while the page is hidden: only the self-reveal step runs (§4.2). */
export const HIDDEN_POLL = 60;
/**
 * Seconds until the next poll (§4.2): 10 s while a march or a transaction is
 * in flight, else 30 s; hidden, the reveal-only tick every 60 s; doubled per
 * consecutive error up to 5 min, hidden or not.
 */
export function pollDelay({ hidden = false, inFlight = false, errors = 0 }) {
  const base = hidden ? HIDDEN_POLL : nextPoll({ kind: 'own', inFlight, errors: 0 });
  return errors > 0 ? Math.min(300, base * 2 ** errors) : base;
}
/** The provinces (other than `home`) where /h/me lists a host of the viewer: `[{p, q}]`. */
export function hostProvinces(meRecord, home = null) {
  const out = [];
  for (const h of meRecord?.hosts ?? []) {
    const [p, q] = Array.isArray(h.province) ? h.province : [];
    if (!Number.isInteger(p) || !Number.isInteger(q)) continue;
    if (home && home.p === p && home.q === q) continue;
    if (!out.some(x => x.p === p && x.q === q)) out.push({ p, q });
  }
  return out;
}
/**
 * The seed source a SettleExplore/SettleTicket may name for a bell-region
 * record: `{archived: true}` once archived, else `{nonce}` of a cache still
 * present whose round is THE anchor's S and whose A is THE anchor's; null
 * while neither exists (wave-3 review, W3-F: not `caches[0]`).
 */
export function seedSourceOf(record) {
  if (!record) return null;
  if (record.archived) return { archived: true };
  const a = record.anchor?.A;
  const c = (record.caches ?? []).find(x => x.present !== false && (record.S === undefined || record.S === null || String(x.round) === String(record.S)) && (a === undefined || String(x.A) === String(a)));
  return c ? { nonce: c.nonce } : null;
}
/** Whether a join needs an invite: the season's join gate is set (I-51). */
export const inviteRequired = season => !!season?.joinGate?.some?.(x => x !== 0);

// ------------------------------------------------------------------ refresh
const Buffer2hex = b => Array.from(b, x => x.toString(16).padStart(2, '0')).join('');
let herald = null, refreshTimer = null, cursor = 0;

function refreshSoon(ms = 1500) {
  clearTimeout(refreshTimer);
  refreshTimer = setTimeout(() => refresh().catch(e => console.error('frontier refresh:', e)), ms);
}

async function loadEnvelope(p, q, bell = 'latest') {
  const r = await herald.province(p, q, bell);
  if (r.ok) { if (bell === 'latest') FS.provinces.set(`${p},${q}`, r); return r; }
  return null;
}

/**
 * The ArrivalSlot of `host` in its destination's envelope of bell `b`: the
 * per-bell file, or the latest one while it is still that bell's; `env`
 * null otherwise (SettleTransit is offered only with the arrival bell's own
 * envelope: its slot and resolver; wave-3 review, W3-F).
 */
async function slotOf(dest, b, hostId) {
  let env = await herald.province(dest.p, dest.q, b);
  if (!env.ok) env = await herald.province(dest.p, dest.q, 'latest');
  if (!env.ok || env.bell !== b) return { env: null, slot: null };
  const s = env.slots.find(x => BigInt(x.account.hostId) === BigInt(hostId));
  return { env, slot: s ?? null };
}

/** Pipeline facts of bell b in region r from the herald's bell-region record. */
async function bellFacts(b, region, resolvedNext = null) {
  const r = await herald.bellRegion(b, region);
  if (!r.ok) return { facts: { anchor: null, archive: null, seed: false, resolvedNext }, record: null };
  const j = r.record;
  return { record: j, facts: { anchor: r.anchor ? { a: Number(r.anchor.a) } : null, archive: j.archived || j.tombstoned ? { archived: !!j.archived, tombstoned: !!j.tombstoned } : null,
    seed: (j.caches ?? []).length > 0, latestRound: 0, resolvedNext } };
}

async function refreshMarches(holding) {
  const key = bookKey();
  if (!key) return;
  let entries = book.loadBook(book.browserStorage, key);
  // Wave-5 review: a gathered transit carries its destination (FLAG_GATHERED,
  // v1.7), so a card with no local entry still names its region.
  const transits = holding ? holding.transit.map((t, slot) => ({ slot, state: t.state, hostId: t.hostId, arriveBell: t.arriveBell, sealRoot: Buffer2hex(t.sealRoot),
    dest: (t.flags & 4) ? { p: t.destP, q: t.destQ } : null })) : [];
  const marches = [];
  const revealed = [];
  const b = nowBell();
  for (const e of entries.filter(x => x.state !== 'settled' && x.state !== 'failed')) {
    const p = unpack(book.materialBytes(e).plain);
    const dest = { p: p.destP, q: p.destQ };
    const region = regionOf(dest.p, dest.q);
    const { slot, env } = b >= e.arriveBell ? await slotOf(dest, e.arriveBell, e.host) : { slot: null, env: null };
    if (slot) revealed.push(e.host);
    const { facts, record } = b >= e.arriveBell ? await bellFacts(e.arriveBell, region, env?.province.resolvedNext ?? null) : { facts: {}, record: null };
    const pl = pipeline(FS.clock, e.arriveBell, { now: now(), ...facts });
    const transit = transits.find(t => t.slot === e.transitSlot && String(t.hostId) === e.host) ?? null;
    // Self-reveal (§9.1): at the arrival bell's start + the drawn delay; once more after 60 s if no slot and the window is open.
    const step = book.revealStep(e, { now: now(), bellStart: bellStart(FS.clock.genesisTs, e.arriveBell), windowOpen: pl.state === 'open' || pl.state === 'awaitingBeacon' || pl.state === 'revealing', slotPresent: !!slot });
    if (step.action === 'send') {
      const r = await io.reveal(book.revealMaterial(e));
      entries = book.recordAttempt(entries, e, { t: now(), route: 'self', result: r.ok ? 'accepted' : r.code });
      book.saveBook(book.browserStorage, key, entries);
    }
    const settleReady = !!transit && !!env && transit.state >= 2 && pl.state === 'resolved' && pl.close !== null && now() >= pl.close + 600;
    marches.push({ entry: entries.find(x => x.host === e.host && x.arriveBell === e.arriveBell), transit, facts: { nowBell: b, pipeline: pl.state, slotPresent: !!slot, settleReady }, record, slot, env, dest, region });
  }
  // A live transit this device has no entry for (another device, cleared
  // storage): a card with entry null, so the tracker says keepers reveal it.
  for (const t of transits.filter(x => x.state >= 1 && x.state <= 3)) {
    if (marches.some(m => m.transit && m.transit.slot === t.slot)) continue;
    if (entries.some(e => e.transitSlot === t.slot && e.host === String(t.hostId) && e.state !== 'settled')) continue;
    marches.push({ entry: null, transit: t, facts: { nowBell: b, pipeline: null, slotPresent: false, settleReady: false }, record: null, slot: null, env: null,
      dest: t.dest, region: t.dest ? regionOf(t.dest.p, t.dest.q) : null });
  }
  entries = book.reconcile(entries, { transits, revealedHosts: revealed, complete: !!holding });
  book.saveBook(book.browserStorage, key, entries);
  FS.book = entries;
  FS.marches = marches;
  FS.bellItems = [
    ...(holding ? [{ bell: b, region: regionOf(holding.p, holding.q), why: 'current', facts: {} }] : []),
    ...marches.map(m => ({ bell: m.entry?.arriveBell ?? m.transit?.arriveBell, region: m.region, why: 'arrival', facts: {} })),
  ];
}

async function refreshChronicle() {
  if (cursor === 0 && FS.record?.headSeq !== undefined) cursor = chronicleStart(FS.record.headSeq);
  for (let page = 0; page < CHRONICLE_PAGES; page++) {
    const r = await herald.events(cursor);
    if (!r.ok) break;
    const { records } = decodePage(r.events);
    FS.chronicle = [...(FS.chronicle ?? []), ...records].slice(-400);
    if (r.next !== null && r.next !== undefined) cursor = r.next;
    if (!r.full) break;
  }
  const own = FS.holdings ?? [];
  FS.isMine = rec => {
    if (rec.host_id !== undefined) { const h = hostParts(rec.host_id); return !!h && own.some(o => o.p === h.p && o.q === h.q && o.site === h.site); }
    return rec.p !== undefined && own.some(o => o.p === rec.p && o.q === rec.q && (rec.site === undefined || rec.site === o.site));
  };
  // Incoming (§7.7): public departures from other factions' holdings.
  const departures = FS.chronicle.filter(x => x.record.name === 'DEPART').map(x => {
    const h = hostParts(x.record.host_id);
    const ov = h && [...FS.overviews.values()].flatMap(o => o.provinces).find(pr => pr.p === h.p && pr.q === h.q);
    return { ...x.record, faction: ov && h ? ov.owners[h.site] : null };
  }).filter(d => d.faction !== null && d.faction < 6);
  const k = FS.kernel ?? null;
  FS.incoming = FS.citizen ? incomingWarnings({ kernel: k, departures, holdings: own.map(o => ({ p: o.p, q: o.q, site: o.site, tile: o.tile })), faction: FS.citizen.faction, genesisTs: FS.clock.genesisTs, nowBell: nowBell(), unitOf: () => WARNING_UNIT }) : [];
}

/** The herald client this controller reads (startPlay sets it; tests pass a fake). */
export function useHerald(h) { herald = h; cursor = 0; }

/** Refresh the viewer's accounts, provinces, marches, chronicle and quota. */
export async function refresh() {
  if (!herald || !FS.clock) return;
  FS.nowBell = nowBell();
  // A gated season shows the invite field before any refusal (I-51).
  FS.inviteRequired = FS.inviteRequired || inviteRequired(FS.season);
  if (FS.wallet) {
    const me = await herald.me(FS.wallet.address);
    if (me.ok) {
      const before = FS.land?.stage;
      FS.citizen = me.citizen;
      FS.holdings = me.holdings;
      FS.land = landState(me.citizen);
      if (before === 'provisional' && FS.land.stage === 'joined') FS.hadHolding = true;
      if (FS.ui?.ticketSeen === false && FS.land.stage !== 'joined' && FS.land.stage !== 'none') saveUiPatch({ ticketSeen: true });
      if (me.record?.quota) FS.quota = me.record.quota;
      if (FS.session && FS.citizen) {
        const m = fsession.matchesCitizen(FS.session, FS.citizen, now());
        FS.sessionProblem = m.ok ? null : m.code;
      }
    } else if (me.code === 'NotFound') {
      FS.citizen = null; FS.holdings = []; FS.land = landState(null);
    }
    const h = FS.holdings?.[0];
    if (h) await loadEnvelope(h.p, h.q);
    const home = h ? FS.provinces.get(`${h.p},${h.q}`)?.province ?? null : null;
    if (shouldAutoNudge({ holding: h, province: home, nowBell: FS.nowBell, lastBell: FS.autoNudgeBell ?? null, hidden: !!globalThis.document?.hidden })) {
      FS.autoNudgeBell = FS.nowBell;
      io.nudge(h.p, h.q, home.resolvedNext).then(r => { if (r.ok) refreshSoon(4000); }).catch(() => {});
    }
    // Hosts standing in other provinces (after a march that Stays): their
    // envelopes too, so the Hosts tab can show and command them.
    if (me.ok) for (const pq of hostProvinces(me.record, h ? { p: h.p, q: h.q } : null)) await loadEnvelope(pq.p, pq.q);
    // SettleExplore is offered once the explore bell's seed is readable.
    FS.exploreSeedReady = false;
    if (h?.explore?.state && nowBell() >= h.explore.bell) {
      const r = await herald.bellRegion(h.explore.bell, regionOf(h.explore.p, h.explore.q));
      FS.exploreSeedReady = r.ok && !!seedSourceOf(r.record);
    }
    await refreshMarches(h ?? null);
    if (FS.citizen && io.pinned()) {
      const q = await io.quota(io.pinned().addresses.of('Citizen', { wallet: FS.wallet.address }));
      if (q.ok) FS.quota = q;
    }
  }
  await refreshChronicle();
  invalidate('panel', 'tabs', 'chips', 'map');
}

// ------------------------------------------------------------------ the composer
async function kernelOrNull() {
  if (FS.kernel) return FS.kernel;
  try { FS.kernel = await loadKernel(); FS.kernelError = null; } catch (e) { FS.kernelError = e.code ?? 'NoWasm'; FS.kernel = null; }
  return FS.kernel;
}

async function setDestination(dest) {
  const c = FS.compose;
  if (!c) return;
  c.dest = dest; c.route = null; c.routeError = null; c.earliest = null;
  invalidate('panel');
  const k = await kernelOrNull();
  if (!k) { c.routeError = FS.kernelError ?? 'NoKernel'; invalidate('panel'); return; }
  const seeds = (FS.record?.rings ?? []).map(r => ({ ring: r.d, seed: r.seed }));
  const r = planRoute(k, { from: { p: c.origin.p, q: c.origin.q, tile: c.host.tile }, to: dest, unit: c.host.unit, seeds });
  if (!r.ok) { c.routeError = r.code; invalidate('panel'); return; }
  c.route = r.route;
  c.earliest = earliestBell(k, { genesisTs: FS.clock.genesisTs, departTs: now() + DEPART_MARGIN_SECS, secs: r.route.secs });
  c.arriveBell = arrivalWindow(FS.season, nowBell(), c.earliest).min;
  invalidate('panel');
}

function quickDestinations() {
  const out = [];
  for (const env of FS.provinces.values()) {
    const camp = env.province.camp;
    if (camp?.state === 1) out.push({ kind: 'camp', p: env.province.p, q: env.province.q, tile: camp.tile });
  }
  for (const h of FS.holdings ?? []) out.push({ kind: 'holding', p: h.p, q: h.q, tile: h.tile });
  return out;
}

async function sendTheMarch() {
  const c = FS.compose, h = FS.holdings[0];
  const pin = io.pinned();
  const env = FS.provinces.get(`${c.origin.p},${c.origin.q}`);
  const m = { season: FS.season, nowBell: nowBell(), resolvedNext: env?.province.resolvedNext ?? null, province: env?.province ?? null, now: now(), holding: h, host: c.host, dest: c.dest, route: c.route,
    earliest: c.earliest, arriveBell: c.arriveBell, stance: c.stance, retreat: { choice: c.retreat, ratio: c.ratio }, tip: c.tip,
    holdingAddress: pin.addresses.of('Holding', h) };
  c.sending = true; c.step = null;
  invalidate('panel');
  FS.inFlight = (FS.inFlight ?? 0) + 1;
  try {
    const r = await sendMarch(m, {
      seal: req => sealMarch(req), clock: FS.clock, publicKey: FS.beacon?.publicKey,
      storage: book.browserStorage, bookKey: bookKey(), session: FS.session,
      accounts: accountsFor('Depart', ctx({ holding: { p: h.p, q: h.q, site: h.site }, province: c.origin })),
      submit, track, onStep: s => { c.step = s; invalidate('panel'); },
      earliestAtSend: () => (FS.kernel && c.route ? earliestBell(FS.kernel, { genesisTs: FS.clock.genesisTs, departTs: now() + DEPART_MARGIN_SECS, secs: c.route.secs }) : null),
    });
    if (r.ok) { done(() => L`出発しました`); FS.compose = null; FS.tab = 'marches'; } else {
      // A stale earliest bell: show the new window (the player chooses again).
      if (r.code === 'ArrivalBell' && r.earliest !== null && r.earliest !== undefined) { c.earliest = r.earliest; c.arriveBell = Math.max(c.arriveBell, arrivalWindow(FS.season, nowBell(), r.earliest).min); }
      failed(r);
    }
  } finally {
    c.sending = false;
    FS.inFlight -= 1;
    refreshSoon();
  }
}

// ------------------------------------------------------------------ settlement (settle shapes: no authority signer, charged to the requester)
async function settleExplore() {
  const h = FS.holdings[0], rec = h.explore;
  const region = regionOf(rec.p, rec.q);
  const r = await herald.bellRegion(rec.bell, region);
  const A = io.pinned().addresses;
  const src = r.ok ? seedSourceOf(r.record) : null;
  if (!src) return failed({ code: 'SeedNotReady' });
  const archived = !!src.archived;
  const nonce = src.nonce;
  const archive = A.of('AnchorArchive', { region, part: archivePartOf(rec.bell) });
  const settle = archived ? { seed: archive, anchor: archive } : { seed: A.of('SeedCache', { bell: rec.bell, region, nonce }), anchor: A.of('BellAnchor', { bell: rec.bell, region }) };
  return act('SettleExplore', { v: ctx({ holding: h, settle }), signer: null, requester: FS.session });
}

/**
 * The Citizen a SettleTransit must name when this host won the camp (v1.7,
 * I-56; the keeper's rule, fclient camp_winner): the host's arrival record
 * in the resolved ClashInputs Stays at the lowest `camp_mask` position. The
 * mask and the fate come from the arrival bell's own envelope (decoded
 * `campMask`, ABI v1.8); the herald's clash report bytes are the fallback. The Holding's owner Citizen, or null.
 */
export async function campCitizenOf(m, hostId, { heraldClient = herald, holding = FS.holdings?.[0], faction = FS.citizen?.faction } = {}) {
  const ci = m.env?.inputs;
  if (!ci || !m.dest || !m.transit || !holding || !Number.isInteger(faction)) return null;
  const k = [0, 1, 2, 3].find(i => { const r = ci.arrivals?.[faction * 4 + i]; return r?.present === 1 && String(r.hostId) === String(hostId); });
  if (k === undefined || ci.arrivals[faction * 4 + k].fate !== 1) return null;
  // v1.8: the ABI names ClashInputs' camp_mask (was RSV_76), so the decoded
  // envelope carries it; the raw /h/clash bytes remain the fallback.
  let mask = Number.isInteger(ci.campMask) ? ci.campMask : null;
  if (mask === null) {
    const rep = await heraldClient.clash(m.dest.p, m.dest.q, m.transit.arriveBell);
    const raw = rep?.ok && rep.report?.inputs_b64 ? fromBase64(rep.report.inputs_b64) : null;
    if (!raw) return null;
    mask = campMaskOf(raw);
  }
  if (!campWinner(mask, faction, k, ci.arrivals[faction * 4 + k].fate)) return null;
  return toBase58(holding.ownerCitizen);
}

async function settleTransit(hostId) {
  const m = (FS.marches ?? []).find(x => String(x.entry?.host) === String(hostId));
  const h = FS.holdings[0];
  if (!m?.entry?.seal_b64 || !m.transit) return failed({ code: 'TransitState' });
  const seal = fromBase64(m.entry.seal_b64);
  const commit = Uint8Array.from(m.entry.commit_hex.match(/../g).map(x => parseInt(x, 16)));
  // The pair judged must be the logged pair: sha256(commit ‖ sha256(seal)) = the transit's seal_root.
  if (Buffer2hex(sealRootOf(commit, ctHashOf(seal))) !== m.transit.sealRoot) return failed({ code: 'CommitMismatch' });
  const A = io.pinned().addresses;
  const slotAcc = m.slot?.account ?? null;
  const inputs = m.env?.inputs ?? null;
  const archive = m.record?.archived;
  const anchor = archive ? A.of('AnchorArchive', { region: m.region, part: archivePartOf(m.transit.arriveBell) }) : A.of('BellAnchor', { bell: m.transit.arriveBell, region: m.region });
  const campCitizen = await campCitizenOf(m, hostId);
  const v = fp => ctx({ holding: { p: h.p, q: h.q, site: h.site }, settle: {
    dest: m.dest, arriveBell: m.transit.arriveBell, faction: FS.citizen.faction, slotIndex: slotAcc?.i ?? 0, anchor,
    slotBeneficiary: slotAcc ? toBase58(slotAcc.beneficiary) : fp, resolver: inputs ? toBase58(inputs.resolver) : fp, rentPayer: toBase58(h.rentPayer), beneficiary: fp, campCitizen } });
  return act('SettleTransit', { v, fields: fp => ({ transit_slot: m.entry.transitSlot, commit, seal, beneficiary: fromBase58(fp) }), signer: null, requester: FS.session });
}

// ------------------------------------------------------------------ actions (data-act)
const num = x => Number.parseInt(x, 10);
const formNum = (form, name) => Number(form?.elements?.[name]?.value ?? NaN);

export const ACTIONS = {
  tab: d => { FS.tab = d.tab; FS.notice = FS.notice?.busy ? FS.notice : null; saveUiPatch({ tab: d.tab }); invalidate('panel', 'tabs'); },
  connect: async d => {
    const entry = (FS.walletList ?? [])[num(d.i)];
    try {
      await wallet.connect(entry, { chain: wallet.chainOf(io.pinned()?.cluster ?? 'localnet') });
    } catch (e) { return failed({ code: e.code ?? 'WalletError', text: e.message }); }
    FS.wallet = wallet.connected();
    FS.session = await fsession.restore(scope(), FS.wallet.address);
    await refresh();
  },
  'pick-faction': d => { FS.joinDraft = { ...(FS.joinDraft ?? {}), faction: num(d.f) }; invalidate('panel'); },
  join: async () => {
    const pin = io.pinned();
    let s;
    try { s = await fsession.derive({ wallet: wallet.connected(), cluster: pin.cluster, programId: pin.programId, seasonId: pin.seasonId }); } catch (e) { return failed({ code: e.code ?? 'SignFailed' }); }
    fsession.remember(s);
    FS.session = s;
    const gate = FS.season.joinGate.some(x => x !== 0) ? toBase58(FS.season.joinGate) : null;
    return act('Join', { v: { faction: FS.joinDraft.faction, joinGate: gate, invite: FS.joinDraft.invite },
      fields: { faction: FS.joinDraft.faction, session: s.publicKeyBytes, session_expiry: BigInt(Math.floor(now()) + SESSION_DAYS - 600) } });
  },
  session: async () => {
    const pin = io.pinned();
    let s;
    try { s = await fsession.derive({ wallet: wallet.connected(), cluster: pin.cluster, programId: pin.programId, seasonId: pin.seasonId }); } catch (e) { return failed({ code: e.code ?? 'SignFailed' }); }
    fsession.remember(s);
    FS.session = s;
    const m = fsession.matchesCitizen(s, FS.citizen, now());
    if (m.ok) { FS.sessionProblem = null; return invalidate('panel'); }
    return act('SetSession', { fields: { session: s.publicKeyBytes, expiry: BigInt(Math.floor(now()) + SESSION_DAYS - 600) } });
  },
  'pick-province': async d => {
    const env = await herald.province(num(d.p), num(d.q));
    FS.joinDraft = { ...(FS.joinDraft ?? {}), envelope: env.ok ? env : null };
    invalidate('panel');
  },
  'toggle-site': d => {
    const s = { p: num(d.p), q: num(d.q), site: num(d.site) };
    const list = FS.joinDraft?.sites ?? [];
    const i = list.findIndex(x => x.p === s.p && x.q === s.q && x.site === s.site);
    FS.joinDraft = { ...FS.joinDraft, sites: i >= 0 ? list.filter((_, j) => j !== i) : list.length < 3 ? [...list, s] : list };
    invalidate('panel');
  },
  'file-ticket': () => fileTicket(FS.joinDraft?.sites ?? []),
  refile: () => fileTicket(FS.ui?.lastTicket ?? []),
  harvest: () => act('Harvest', { v: { holding: FS.holdings[0] } }),
  build: d => act('Build', { v: { holding: FS.holdings[0], item: num(d.item) }, fields: { item: num(d.item) } }),
  // W6-D: re-read soon after the keeper was asked (the next poll could be 30 s away, past the window the catch-up opens).
  nudge: () => { const h = FS.holdings[0], env = FS.provinces.get(`${h.p},${h.q}`); return io.nudge(h.p, h.q, env?.province.resolvedNext ?? nowBell()).then(r => { refreshSoon(4000); return r.ok ? done(() => L`キーパーに追いつくよう頼みました`) : failed(r); }); },
  dissolve: d => { const r = hostRow(d.host); return act('Dissolve', { v: { holding: FS.holdings[0], province: r ? { p: r.p, q: r.q } : null }, fields: { host_id: BigInt(d.host) } }); },
  compose: d => {
    const r = hostRow(d.host);
    if (!r) return;
    const tips = tipOptions(FS.season);
    FS.compose = { host: { ...r.entry, id: r.id, troops: r.troops, tile: r.tile, unit: r.unit }, origin: { p: r.p, q: r.q }, stance: 0, retreat: 'never', ratio: null,
      tip: String(tips[0].lamports), quick: quickDestinations(), dest: null, route: null, arriveBell: arrivalWindow(FS.season, nowBell()).min };
    FS.tab = 'marches';
    invalidate('panel', 'tabs');
  },
  'compose-close': () => { FS.compose = null; invalidate('panel'); },
  'dest-from-map': () => { const s = FS.selected; if (s?.idx !== undefined) return setDestination({ p: s.p, q: s.q, tile: s.idx }); return undefined; },
  'dest-quick': d => setDestination({ p: num(d.p), q: num(d.q), tile: num(d.tile) }),
  'march-send': () => sendTheMarch(),
  'explore-open': d => { const r = hostRow(d.host); FS.explore = r ? { host: r, tiles: [] } : null; FS.tab = 'hosts'; invalidate('panel', 'tabs'); },
  'explore-tile': d => { FS.explore.tiles = toggleTile(FS.explore.tiles, num(d.tile)); invalidate('panel'); },
  'explore-send': () => {
    const x = FS.explore;
    return act('Explore', { v: { holding: FS.holdings[0], province: { p: x.host.p, q: x.host.q } },
      fields: { host_id: BigInt(x.host.id), n: x.tiles.length, tiles: Uint8Array.of(x.tiles[0], x.tiles[1] ?? 0) }, after: () => { FS.explore = null; } });
  },
  'settle-explore': () => settleExplore(),
  'settle-transit': d => settleTransit(d.host),
  fog: () => { FS.view.fog = !FS.view.fog; saveUiPatch({ fog: FS.view.fog }); invalidate('map', 'panel'); },
  forget: () => { if (FS.session) fsession.forget(FS.session); FS.session = null; invalidate('panel'); },
};

/** Form submissions (data-form). */
export const FORMS = {
  train: f => act('Train', { v: { holding: FS.holdings[0] }, fields: { unit: formNum(f, 'unit'), n: formNum(f, 'n') } }),
  muster: f => act('Muster', { v: { holding: FS.holdings[0] }, fields: { unit: formNum(f, 'unit'), troops: formNum(f, 'troops'), tile: FS.holdings[0].tile } }),
  garrison: f => act('Garrison', { v: { holding: FS.holdings[0] }, fields: { delta: BigInt(Math.trunc(formNum(f, 'delta'))) } }),
  vigil: f => act('SetVigil', { fields: { start_min: Math.max(0, Math.min(23, Math.trunc(formNum(f, 'hour')))) * 60 } }),
  dest: f => setDestination({ p: Math.trunc(formNum(f, 'p')), q: Math.trunc(formNum(f, 'q')), tile: Math.trunc(formNum(f, 'tile')) }),
};

/** Bound inputs (data-bind) of the composer and the join draft. */
export function bind(name, value) {
  if (name === 'invite') { FS.joinDraft = { ...(FS.joinDraft ?? {}), invite: String(value).trim() }; return; }
  const c = FS.compose;
  if (!c) return;
  if (name === 'arriveBell') c.arriveBell = num(value);
  else if (name === 'stance') c.stance = num(value);
  else if (name === 'retreat') c.retreat = String(value);
  else if (name === 'ratio') c.ratio = Number(value);
  else if (name === 'tip') c.tip = String(value);
  invalidate('panel');
}

function hostRow(id) {
  const h = FS.holdings?.[0];
  if (!h) return null;
  for (const env of FS.provinces.values()) {
    const r = hostsIn(env.province, h, nowBell()).find(x => String(x.id) === String(id));
    if (r) return { ...r, p: env.province.p, q: env.province.q, province: env.province };
  }
  return null;
}

async function fileTicket(sites) {
  if (!sites.length) return undefined;
  saveUiPatch({ lastTicket: sites, ticketSeen: false, lastTicketBell: nowBell() });
  FS.hadHolding = false;
  return act('FileTicket', { v: { sites }, fields: { sites } });
}

// ------------------------------------------------------------------ start
/**
 * Start play on the game page: `{herald, cfg}`. Wallet discovery (the dev
 * wallet only on a localnet season served from a loopback page), the UI
 * preferences, the session key kept on this device, and the poll loop.
 */
export async function startPlay({ herald: h, cfg }) {
  useHerald(h);
  const pin = io.pinned();
  if (pin) herald.pin(pin.seasonId, pin.addresses);
  FS.ui = scope() ? loadUi(uiStorage, uiKey(scope())) : loadUi(uiStorage, '');
  FS.view.fog = FS.ui.fog;
  FS.tab = FS.ui.tab;
  wallet.discover();
  if (cfg.dev && pin?.cluster === 'localnet') await wallet.enableDevWallet({ cluster: 'localnet', allowed: true }).catch(() => false);
  const listWallets = () => { FS.walletList = wallet.list(wallet.chainOf(pin?.cluster ?? 'localnet')); invalidate('panel'); };
  wallet.onChange(listWallets);
  listWallets();
  // A silent reconnect of the wallet used last time (no popup; nothing if it does not trust this page).
  const last = wallet.lastWalletName();
  const entry = last && (FS.walletList ?? []).find(w => w.name === last && !w.why);
  if (entry) {
    const c = await wallet.connect(entry, { chain: wallet.chainOf(pin?.cluster ?? 'localnet'), silent: true }).catch(() => null);
    if (c) { FS.wallet = wallet.connected(); FS.session = await fsession.restore(scope(), FS.wallet.address); }
  }
  // The rules module once (the incoming warnings need it too); a missing
  // frontier.wasm is tolerated (the page says what needs it).
  await kernelOrNull();
  await refresh();
  let errors = 0;
  const hidden = () => !!globalThis.document?.hidden;
  const loop = async () => {
    try {
      // Hidden: only the marchbook's self-reveal step (§4.2), not the full refresh.
      if (hidden()) await revealTick(); else await refresh();
      errors = 0;
    } catch (e) { errors++; console.error('frontier refresh:', e); }
    const wait = pollDelay({ hidden: hidden(), inFlight: (FS.inFlight ?? 0) > 0 || (FS.marches ?? []).length > 0, errors });
    setTimeout(loop, wait * 1000);
  };
  globalThis.document?.addEventListener?.('visibilitychange', () => { if (!hidden()) refreshSoon(0); });
  setTimeout(loop, 10_000);
}

/** The hidden page's tick: the self-reveal of the marchbook's due marches only. */
async function revealTick() {
  if (!herald || !FS.clock || !FS.wallet || !(FS.book ?? []).some(e => e.state !== 'settled' && e.state !== 'failed')) return;
  await refreshMarches(FS.holdings?.[0] ?? null);
}

