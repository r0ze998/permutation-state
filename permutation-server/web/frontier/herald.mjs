// The read path: the herald's cached files and WS diffs (contract §8.4,
// §9.2, §9.3; web design §4). The herald is never a trust root: every
// record carries raw account bytes, which the client decodes with its own
// codec (fcodec.mjs) and checks against the key it asked for and the
// pinned season; the herald's decoded JSON is never used for anything the
// client signs or verifies.
//
// Many viewers (§4.2): only what is on screen is fetched; immutable
// per-bell files are cached (LRU, 256 province records); "latest" files
// are not; own entities poll every 10 s while a march or a transaction is
// in flight, else 30 s; overviews once per bell with a 0–15 s jitter after
// the bell; errors back off ×2 up to 5 min; a hidden page pauses polling.
import { fromBase64 } from '../sdk/bytes.mjs';
import { encode as toB58 } from '../sdk/base58.mjs';
import { decode } from './fcodec.mjs';

// ------------------------------------------------------------------ HTTP
/** A failed read (never thrown). */
const fail = (code, error, httpStatus = 0) => ({ ok: false, code, error: error ?? code, httpStatus });

async function get(f, url, { binary = false, timeoutMs = 12_000 } = {}) {
  let r;
  try {
    r = await f(url, { cache: 'no-store', signal: AbortSignal.timeout?.(timeoutMs) });
  } catch {
    return fail('network');
  }
  if (!r.ok) return fail(r.status === 404 ? 'NotFound' : r.status === 425 ? 'TooEarly' : `HTTP${r.status}`, `HTTP ${r.status}`, r.status);
  try {
    return binary ? { ok: true, bytes: new Uint8Array(await r.arrayBuffer()) } : { ok: true, json: await r.json() };
  } catch {
    return fail('BadBody', 'the herald answered something unreadable', r.status);
  }
}

// ------------------------------------------------------------------ small LRU
export class Lru {
  constructor(max) { this.max = max; this.m = new Map(); }
  get(k) { if (!this.m.has(k)) return undefined; const v = this.m.get(k); this.m.delete(k); this.m.set(k, v); return v; }
  set(k, v) { this.m.delete(k); this.m.set(k, v); while (this.m.size > this.max) this.m.delete(this.m.keys().next().value); }
  get size() { return this.m.size; }
}

// ------------------------------------------------------------------ record checks
const bytesOf = b64 => (typeof b64 === 'string' ? fromBase64(b64) : null);
const KEY = {
  pv: /^pv:(-?\d+),(-?\d+)$/,
  ar: /^ar:(-?\d+),(-?\d+),(\d+),(\d+),(\d+)$/,
  ad: /^ad:(-?\d+),(-?\d+),(\d+)$/,
  ci: /^ci:(-?\d+),(-?\d+),(\d+)$/,
};

export class HeraldError extends Error {
  constructor(code, message) { super(message); this.code = code; }
}
const check = (cond, code, msg) => { if (!cond) throw new HeraldError(code, msg); };

/**
 * A province envelope (§9.2) → decoded accounts, each checked against its
 * key and the season: `{bell, slot, seq, head, province, slots: [{key,
 * slot, account}], day, inputs}`. With `bell` (a per-bell file, not
 * `latest`), the envelope, every slot, the day and the inputs must be that
 * bell's (W3-F, the deferred envelope-bell check of integ-W2). Throws
 * HeraldError on any mismatch.
 */
export function parseEnvelope(env, { seasonId, p, q, bell } = {}) {
  check(env && env.v === 1, 'BadEnvelope', 'not a v1 envelope');
  const m = KEY.pv.exec(env.key ?? '');
  check(m, 'BadEnvelope', `key ${env.key}`);
  const [P, Q] = [+m[1], +m[2]];
  if (p !== undefined) check(P === p && Q === q, 'WrongKey', `asked for pv:${p},${q}, got ${env.key}`);
  const province = decode('Province', bytesOf(env.bytes) ?? new Uint8Array(0), { seasonId });
  check(province.p === P && province.q === Q, 'WrongKey', 'the Province bytes are another province');
  const slots = (env.slots ?? []).map(s => {
    const k = KEY.ar.exec(s.key ?? '');
    check(k, 'BadEnvelope', `slot key ${s.key}`);
    const a = decode('ArrivalSlot', bytesOf(s.bytes), { seasonId });
    check(a.p === +k[1] && a.q === +k[2] && a.bell === +k[3] && a.faction === +k[4] && a.i === +k[5], 'WrongKey', `slot bytes are not ${s.key}`);
    check(a.p === P && a.q === Q, 'WrongKey', 'a slot of another province');
    return { key: s.key, slot: s.slot, account: a };
  });
  let day = null;
  if (env.day) {
    const k = KEY.ad.exec(env.day.key ?? '');
    check(k, 'BadEnvelope', `day key ${env.day.key}`);
    day = decode('ArrivalDay', bytesOf(env.day.bytes), { seasonId });
    check(day.p === +k[1] && day.q === +k[2] && day.day === +k[3] && day.p === P && day.q === Q, 'WrongKey', 'day bytes');
  }
  let inputs = null;
  if (env.inputs) {
    const k = KEY.ci.exec(env.inputs.key ?? '');
    check(k, 'BadEnvelope', `inputs key ${env.inputs.key}`);
    inputs = decode('ClashInputs', bytesOf(env.inputs.bytes), { seasonId });
    check(inputs.p === +k[1] && inputs.q === +k[2] && inputs.bell === +k[3] && inputs.p === P && inputs.q === Q, 'WrongKey', 'inputs bytes');
  }
  if (Number.isInteger(bell)) {
    check(env.bell === bell, 'WrongKey', `asked for bell ${bell}, got ${env.bell}`);
    check(slots.every(s => s.account.bell === bell), 'WrongKey', 'a slot of another bell');
    check(!day || day.day === Math.floor(bell / 144), 'WrongKey', 'the day of another bell');
    check(!inputs || inputs.bell === bell, 'WrongKey', 'the inputs of another bell');
  }
  return { bell: env.bell, slot: env.slot, seq: String(env.seq ?? ''), head: env.head ?? null, province, slots, day, inputs };
}

// ------------------------------------------------------------------ the overview binary (§9.3)
export const OVERVIEW_MAGIC = 'PSFOV1\0\0';
export const OVERVIEW_HEADER = 32;
export const OVERVIEW_RECORD = 24;
/** Site states in the overview: free, holding, camp, reserved/released. */
export const SITE_STATE = Object.freeze(['free', 'holding', 'camp', 'reserved']);

/**
 * `/h/overview/{ring}/{bell}.bin` → `{season, ring, bell, slot, provinces:
 * [{p, q, owners[12] (0–5 faction, 6 neutral/camp, 7 none), sites[12]
 * (SITE_STATE index), hosts[7], clash, dormant, opened, resolvedNext}]}`.
 * Throws HeraldError on a bad magic, length, ring or order.
 */
export function decodeOverview(bytes, { seasonId, ring } = {}) {
  const b = Uint8Array.from(bytes);
  check(b.length >= OVERVIEW_HEADER, 'BadOverview', 'short header');
  check(String.fromCharCode(...b.subarray(0, 8)) === OVERVIEW_MAGIC, 'BadOverview', 'magic');
  const dv = new DataView(b.buffer);
  const out = { season: dv.getBigUint64(8, true), ring: dv.getUint16(16, true), n: dv.getUint16(18, true), bell: dv.getUint32(20, true), slot: dv.getBigUint64(24, true) };
  if (seasonId !== undefined) check(out.season === BigInt(String(seasonId)), 'WrongSeason', 'overview of another season');
  if (ring !== undefined) check(out.ring === ring, 'WrongKey', `asked ring ${ring}, got ${out.ring}`);
  check(b.length === OVERVIEW_HEADER + out.n * OVERVIEW_RECORD, 'BadOverview', 'length');
  out.provinces = [];
  for (let i = 0; i < out.n; i++) {
    const o = OVERVIEW_HEADER + i * OVERVIEW_RECORD;
    let owners = 0n;
    for (let j = 4; j >= 0; j--) owners = (owners << 8n) | BigInt(b[o + 4 + j]);
    const sites = b[o + 9] | (b[o + 10] << 8) | (b[o + 11] << 16);
    const flags = b[o + 19];
    const rec = {
      p: dv.getInt16(o, true),
      q: dv.getInt16(o + 2, true),
      owners: Array.from({ length: 12 }, (_, s) => Number((owners >> BigInt(3 * s)) & 7n)),
      sites: Array.from({ length: 12 }, (_, s) => (sites >> (2 * s)) & 3),
      hosts: Array.from(b.subarray(o + 12, o + 19)),
      clash: !!(flags & 1),
      dormant: !!(flags & 2),
      opened: !!(flags & 4),
      resolvedNext: dv.getUint32(o + 20, true),
    };
    const prev = out.provinces[i - 1];
    check(!prev || prev.p < rec.p || (prev.p === rec.p && prev.q < rec.q), 'BadOverview', 'records not sorted by (P, Q)');
    out.provinces.push(rec);
  }
  return out;
}

/** The majority owner of a province's sites (for the world LOD), or null. */
export function majorityOwner(rec) {
  const n = new Array(8).fill(0);
  rec.sites.forEach((s, i) => { if (s === 1) n[rec.owners[i]]++; });
  let best = null;
  for (let f = 0; f < 6; f++) if (n[f] > 0 && (best === null || n[f] > n[best])) best = f;
  return best;
}

// ------------------------------------------------------------------ WS diffs
/**
 * The WS diff stream (§8.4): messages `{seq, kind, key, slot, head,
 * bytes_b64}` in sequence. `accept(msg)` → 'applied' | 'duplicate' |
 * 'gap' (a missed seq: the client resyncs from the files, then the next
 * message starts a new run).
 */
export class DiffStream {
  constructor({ onResync = () => {} } = {}) { this.last = null; this.onResync = onResync; }
  accept(msg) {
    let seq;
    try { seq = BigInt(String(msg?.seq)); } catch { return 'invalid'; }
    if (this.last === null || seq === this.last + 1n) { this.last = seq; return 'applied'; }
    if (seq <= this.last) return 'duplicate';
    this.last = null;
    this.onResync();
    return 'gap';
  }
}

// ------------------------------------------------------------------ polling (§4.2)
export const OWN_POLL_ACTIVE = 10;
export const OWN_POLL_IDLE = 30;
export const BELL_JITTER = 15;
export const BACKOFF_MAX = 300;
export const STALE_AFTER = 60;
export const PROVINCE_LIMIT_TILE = 12;

/**
 * Seconds until the next poll of one kind, or null while paused:
 *   own:      10 s while a march or a transaction is in flight, else 30 s
 *   overview: at the next bell's end + a random 0–15 s
 * doubled per consecutive error, up to 5 min; a hidden page pauses all.
 */
export function nextPoll({ kind, hidden = false, inFlight = false, errors = 0, secondsToBellEnd = 600, random = Math.random }) {
  if (hidden) return null;
  const base = kind === 'own' ? (inFlight ? OWN_POLL_ACTIVE : OWN_POLL_IDLE) : Math.max(0, secondsToBellEnd) + random() * BELL_JITTER;
  return errors > 0 ? Math.min(BACKOFF_MAX, base * 2 ** errors) : base;
}

/**
 * The province records to hold: the viewer's own (holdings, marches) always,
 * plus the visible ones at tile LOD, at most 12 visible at once.
 */
export function wantedProvinces({ visible = [], own = [], lod = 'world', limit = PROVINCE_LIMIT_TILE }) {
  const out = new Map();
  for (const p of own) out.set(`${p.p},${p.q}`, p);
  if (lod === 'tile') for (const p of visible.slice(0, limit)) out.set(`${p.p},${p.q}`, p);
  return [...out.values()];
}

/** How far the herald's view is behind the chain clock; `stale` past 60 s (actions that need fresh state ask to reload). */
export function staleness({ latestUnix, chainNow, behind: measured }) {
  // `behind` (ChainClock.behind(), measured against the local clock) wins:
  // an estimate built from the herald itself cannot see the herald stall.
  if (measured !== undefined && measured !== null) return { behind: measured, stale: measured > STALE_AFTER };
  if (latestUnix === null || latestUnix === undefined || chainNow === null || chainNow === undefined) return { behind: null, stale: false };
  const behind = Math.max(0, chainNow - Number(latestUnix));
  return { behind, stale: behind > STALE_AFTER };
}

// ------------------------------------------------------------------ `me` key checks
/**
 * The viewer's own record must be the viewer's (W3-F, the deferred `me`
 * check of integ-W2): the Citizen at the canonical address of `wallet` and
 * naming it; each Holding at its canonical address, owned by that Citizen
 * and one of the Citizen's holdings; at most three. Throws HeraldError.
 */
export function checkMe(addresses, wallet, record, citizen, holdings) {
  const want = addresses.of('Citizen', { wallet });
  if (citizen) {
    check(toB58(citizen.wallet) === wallet, 'WrongKey', 'the Citizen names another wallet');
    if (record.citizen?.address !== undefined) check(record.citizen.address === want, 'WrongKey', 'the Citizen is not at its canonical address');
  }
  check(holdings.length <= 3, 'BadRecord', 'more than three holdings');
  check(!holdings.length || citizen, 'BadRecord', 'holdings without a Citizen');
  holdings.forEach((h, i) => {
    check(toB58(h.ownerCitizen) === want, 'WrongKey', 'a Holding of another Citizen');
    const at = addresses.of('Holding', { p: h.p, q: h.q, site: h.site });
    if (record.holdings?.[i]?.address !== undefined) check(record.holdings[i].address === at, 'WrongKey', 'a Holding not at its canonical address');
    check(citizen.holding.some(r => r.p === h.p && r.q === h.q && r.site === h.site), 'WrongKey', 'a Holding the Citizen does not name');
  });
}

// ------------------------------------------------------------------ the client
/**
 * A herald client for one season. Every call answers `{ok, …}` and never
 * rejects. `seasonId` pins what every decoded account must carry.
 */
export function createHerald({ base = '', fetch: f = (...a) => globalThis.fetch(...a), seasonId = null, cacheSize = 256 } = {}) {
  const root = String(base).replace(/\/+$/, '');
  const cache = new Lru(cacheSize);
  let pinned = seasonId === null ? null : String(seasonId);
  let addrs = null;
  const guard = fn => { try { return { ok: true, ...fn() }; } catch (e) { return fail(e.code ?? 'BadRecord', e.message); } };

  async function cached(key, immutable, load) {
    if (immutable) { const hit = cache.get(key); if (hit) return hit; }
    const r = await load();
    if (r.ok && immutable) cache.set(key, r);
    return r;
  }

  return {
    cache,
    /** Pin the season (and, for the `me` key checks, its recomputed addresses: faddr.seasonAddresses). */
    pin(id, addresses = null) { pinned = String(id); addrs = addresses; },
    /** GET /h/season: the record, with the Season account decoded and checked. */
    async season() {
      const r = await get(f, `${root}/h/season`);
      if (!r.ok) return r;
      return guard(() => {
        const j = r.json;
        check(j && j.v === 1, 'BadRecord', 'not a v1 season record');
        const season = decode('Season', bytesOf(j.bytes_b64) ?? new Uint8Array(0), { seasonId: pinned ?? j.season });
        check(String(season.seasonId) === String(j.season), 'WrongSeason', 'the record and its bytes name different seasons');
        return { record: j, season };
      });
    },
    /** GET /h/province/{P},{Q}/{bell|latest}. */
    province(p, q, bell = 'latest') {
      const immutable = bell !== 'latest';
      return cached(`pv:${p},${q}@${bell}`, immutable, async () => {
        const r = await get(f, `${root}/h/province/${p},${q}/${bell}`);
        return r.ok ? guard(() => parseEnvelope(r.json, { seasonId: pinned ?? undefined, p, q, bell: immutable ? Number(bell) : undefined })) : r;
      });
    },
    /** GET /h/overview/{ring}/{bell|latest}.bin. */
    overview(ring, bell = 'latest') {
      const immutable = bell !== 'latest';
      return cached(`ov:${ring}@${bell}`, immutable, async () => {
        const r = await get(f, `${root}/h/overview/${ring}/${bell}.bin`, { binary: true });
        return r.ok ? guard(() => decodeOverview(r.bytes, { seasonId: pinned ?? undefined, ring })) : r;
      });
    },
    /** GET /h/bell/{bell}/region/{r}: anchor, S, caches, archive state (JSON; the anchor bytes decoded when present). */
    async bellRegion(bell, region) {
      const r = await get(f, `${root}/h/bell/${bell}/region/${region}`);
      if (!r.ok) return r;
      return guard(() => {
        const j = r.json;
        const anchor = j.anchor?.bytes_b64 ? decode('BellAnchor', bytesOf(j.anchor.bytes_b64), { seasonId: pinned ?? undefined }) : null;
        if (anchor) check(anchor.bell === bell && anchor.region === region, 'WrongKey', 'the anchor bytes are another bell or region');
        return { record: j, anchor };
      });
    },
    /** GET /h/me/{wallet}: the Citizen and its Holdings, decoded and checked. */
    async me(wallet) {
      const r = await get(f, `${root}/h/me/${wallet}`);
      if (!r.ok) return r;
      return guard(() => {
        const j = r.json;
        const opts = { seasonId: pinned ?? undefined };
        const citizen = j.citizen?.bytes_b64 ? decode('Citizen', bytesOf(j.citizen.bytes_b64), opts) : null;
        const holdings = (j.holdings ?? []).map(h => decode('Holding', bytesOf(h.bytes_b64), opts));
        if (addrs) checkMe(addrs, wallet, j, citizen, holdings);
        return { record: j, citizen, holdings };
      });
    },
    /** GET /h/events?after={seq}: a page of PS2 records. */
    async events(after = 0) {
      const r = await get(f, `${root}/h/events?after=${encodeURIComponent(String(after))}`);
      return r.ok ? { ok: true, events: Array.isArray(r.json?.events) ? r.json.events : [], next: r.json?.next ?? null } : r;
    },
    /** GET /h/clash/{P},{Q}/{bell}: the clash report (immutable). */
    clash(p, q, bell) {
      return cached(`cl:${p},${q}@${bell}`, true, async () => {
        const r = await get(f, `${root}/h/clash/${p},${q}/${bell}`);
        return r.ok ? guard(() => ({ report: r.json, inputs: r.json?.inputs_b64 ? decode('ClashInputs', bytesOf(r.json.inputs_b64), { seasonId: pinned ?? undefined }) : null })) : r;
      });
    },
  };
}
