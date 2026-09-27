// The herald's read path (contract §8.4, §9.2, §9.3): URL builders, a small
// fetch client and the decoders of its two data formats. The herald is never
// a trust root: an envelope's `bytes` are the account data, which the
// client decodes itself (codec.mjs); decoded conveniences in the answers are
// display hints only.
//
//   GET /h/season                         JSON season record
//   GET /h/overview/{ring}/{bell}.bin     binary overview (below; `latest.bin`)
//   GET /h/province/{P},{Q}/{bell}        JSON envelope (§9.2; `latest`)
//   GET /h/clash/{P},{Q}/{bell}           JSON clash report
//   GET /h/bell/{bell}/region/{r}         JSON bell-region record
//   GET /h/me/{wallet}                    JSON, no-store
//   GET /h/events?after={seq}             JSON pages of PS2 records
//
// Overview (§9.3): header 32 B = magic "PSFOV1\0\0" · season u64 · ring u16 ·
// n u16 · bell u32 · slot u64; then n × 24 B sorted by (P, Q): P i16 · Q i16 ·
// owners 40 bits (12 × 3-bit faction, site i at bits 3i..3i+2: 0–5, 6
// neutral/camp, 7 none) · site_state 24 bits (12 × 2 bits, site i at 2i: 0
// free, 1 holding, 2 camp, 3 reserved/released) · hosts_by_faction [7] u8 ·
// flags u8 (1 clash this bell, 2 dormant holding, 4 opened this bell) ·
// resolved_next u32. All little-endian (the bit fields as LE integers).
import { fromBase64, fromHex } from '../bytes.mjs';

export const OVERVIEW_MAGIC = 'PSFOV1\0\0';
export const OVERVIEW_HEADER = 32;
export const OVERVIEW_RECORD = 24;
export const SITE_STATES = Object.freeze(['free', 'holding', 'camp', 'reserved']);
export const OVERVIEW_FLAGS = Object.freeze({ clash: 1, dormant: 2, opened: 4 });
/** Faction values in `owners`: 0–5 the factions, 6 neutral/camp, 7 nobody. */
export const OWNER_NEUTRAL = 6;
export const OWNER_NONE = 7;

const int = (v, what) => {
  if (!Number.isInteger(Number(v))) throw new TypeError(`${what} must be an integer`);
  return Number(v);
};
const bellPart = bell => (bell === 'latest' ? 'latest' : String(int(bell, 'bell')));

/** Paths of the herald's routes (relative; prefix the herald's base URL). */
export const heraldPaths = Object.freeze({
  season: () => '/h/season',
  overview: (ring, bell = 'latest') => `/h/overview/${int(ring, 'ring')}/${bellPart(bell)}.bin`,
  province: (p, q, bell = 'latest') => `/h/province/${int(p, 'P')},${int(q, 'Q')}/${bellPart(bell)}`,
  clash: (p, q, bell) => `/h/clash/${int(p, 'P')},${int(q, 'Q')}/${int(bell, 'bell')}`,
  bell: (bell, region) => `/h/bell/${int(bell, 'bell')}/region/${int(region, 'region')}`,
  me: wallet => `/h/me/${encodeURIComponent(wallet)}`,
  events: after => `/h/events?after=${encodeURIComponent(String(after ?? 0))}`,
});

/** A key string of the envelope (`pv:<P>,<Q>`, `ar:<P>,<Q>,<b>,<f>,<i>`, `ad:<P>,<Q>,<day>`, `ci:<P>,<Q>,<b>`) parsed. */
export function parseKey(key) {
  const m = /^(pv|ar|ad|ci):(-?\d+(?:,-?\d+)*)$/.exec(String(key));
  if (!m) throw new Error(`bad herald key ${key}`);
  const n = m[2].split(',').map(Number);
  const want = { pv: 2, ar: 5, ad: 3, ci: 3 }[m[1]];
  if (n.length !== want) throw new Error(`bad herald key ${key}`);
  const [p, q, x, f, i] = n;
  return { pv: { kind: 'Province', p, q }, ar: { kind: 'ArrivalSlot', p, q, bell: x, faction: f, i }, ad: { kind: 'ArrivalDay', p, q, day: x },
    ci: { kind: 'ClashInputs', p, q, bell: x } }[m[1]];
}

const decodeHead = h => (h == null ? null : fromHex(h));
const decodeSeq = s => (s == null ? null : BigInt(s));

/**
 * A province envelope (§9.2) with its base64 decoded to bytes and keys
 * parsed: `{v, key, bell, slot, seq, head, bytes, slots: [{key, slot,
 * bytes}], day: {key, bytes}|null, inputs: {key, seq, head, bytes}|null}`.
 */
export function decodeEnvelope(json) {
  if (json?.v !== 1) throw new Error(`herald envelope version ${json?.v}, not 1`);
  return {
    v: 1, key: parseKey(json.key), bell: json.bell, slot: json.slot, seq: decodeSeq(json.seq), head: decodeHead(json.head), bytes: fromBase64(json.bytes),
    slots: (json.slots ?? []).map(s => ({ key: parseKey(s.key), slot: s.slot, bytes: fromBase64(s.bytes) })),
    day: json.day ? { key: parseKey(json.day.key), bytes: fromBase64(json.day.bytes) } : null,
    inputs: json.inputs ? { key: parseKey(json.inputs.key), seq: decodeSeq(json.inputs.seq), head: decodeHead(json.inputs.head), bytes: fromBase64(json.inputs.bytes) } : null,
  };
}

const rd = (b, o, n) => { let v = 0n; for (let i = n - 1; i >= 0; i--) v = (v << 8n) | BigInt(b[o + i]); return v; };
const i16 = (b, o) => { const v = b[o] | (b[o + 1] << 8); return v >= 32768 ? v - 65536 : v; };

/**
 * An overview file decoded: `{season, ring, bell, slot, provinces: [{p, q,
 * owners: [12], sites: [12] ('free'|'holding'|'camp'|'reserved'),
 * hostsByFaction: [7], flags, clash, dormant, opened, resolvedNext}]}`.
 * Throws on a bad magic, a truncated file or unsorted records.
 */
export function decodeOverview(bytes) {
  const b = Uint8Array.from(bytes);
  if (b.length < OVERVIEW_HEADER || String.fromCharCode(...b.subarray(0, 8)) !== OVERVIEW_MAGIC) throw new Error('not an overview file');
  const n = Number(rd(b, 18, 2));
  if (b.length !== OVERVIEW_HEADER + n * OVERVIEW_RECORD) throw new Error(`overview: ${b.length} bytes for ${n} records`);
  const out = { season: rd(b, 8, 8), ring: Number(rd(b, 16, 2)), bell: Number(rd(b, 20, 4)), slot: rd(b, 24, 8), provinces: [] };
  let prev = null;
  for (let k = 0; k < n; k++) {
    const o = OVERVIEW_HEADER + k * OVERVIEW_RECORD;
    const p = i16(b, o);
    const q = i16(b, o + 2);
    if (prev && (p < prev[0] || (p === prev[0] && q <= prev[1]))) throw new Error('overview: records not sorted by (P, Q)');
    prev = [p, q];
    const owners = rd(b, o + 4, 5);
    const sites = rd(b, o + 9, 3);
    const flags = b[o + 19];
    out.provinces.push({
      p, q,
      owners: Array.from({ length: 12 }, (_, i) => Number((owners >> BigInt(3 * i)) & 7n)),
      sites: Array.from({ length: 12 }, (_, i) => SITE_STATES[Number((sites >> BigInt(2 * i)) & 3n)]),
      hostsByFaction: Array.from(b.subarray(o + 12, o + 19)),
      flags, clash: !!(flags & 1), dormant: !!(flags & 2), opened: !!(flags & 4),
      resolvedNext: Number(rd(b, o + 20, 4)),
    });
  }
  return out;
}

/** The inverse of decodeOverview (fixtures and the herald's tests; the herald itself is Rust). */
export function encodeOverview({ season, ring, bell, slot, provinces }) {
  const sorted = [...provinces].sort((a, b) => a.p - b.p || a.q - b.q);
  const out = new Uint8Array(OVERVIEW_HEADER + sorted.length * OVERVIEW_RECORD);
  const put = (o, n, v) => { let x = BigInt(v); for (let i = 0; i < n; i++, x >>= 8n) out[o + i] = Number(x & 0xffn); };
  for (let i = 0; i < 8; i++) out[i] = OVERVIEW_MAGIC.charCodeAt(i);
  put(8, 8, season); put(16, 2, ring); put(18, 2, sorted.length); put(20, 4, bell); put(24, 8, slot);
  sorted.forEach((r, k) => {
    const o = OVERVIEW_HEADER + k * OVERVIEW_RECORD;
    put(o, 2, r.p < 0 ? r.p + 65536 : r.p); put(o + 2, 2, r.q < 0 ? r.q + 65536 : r.q);
    put(o + 4, 5, r.owners.reduce((a, f, i) => a | (BigInt(f & 7) << BigInt(3 * i)), 0n));
    put(o + 9, 3, r.sites.reduce((a, s, i) => a | (BigInt(SITE_STATES.indexOf(s) & 3) << BigInt(2 * i)), 0n));
    r.hostsByFaction.forEach((h, i) => { out[o + 12 + i] = Math.min(255, h); });
    out[o + 19] = r.flags ?? ((r.clash ? 1 : 0) | (r.dormant ? 2 : 0) | (r.opened ? 4 : 0));
    put(o + 20, 4, r.resolvedNext);
  });
  return out;
}

/**
 * A herald client over `fetch` (the browser's, or a test's). `base`: the
 * herald's origin ('' = same origin, as the web page is served by the
 * herald). Answers: parsed JSON, decoded envelopes and overviews; an HTTP
 * failure throws an Error with `status` and `url`.
 */
export class HeraldClient {
  constructor({ base = '', fetch: f = globalThis.fetch } = {}) {
    this.base = base.replace(/\/$/, '');
    this.fetch = f;
  }

  async get(path, { binary = false } = {}) {
    const url = this.base + path;
    const r = await this.fetch(url, { headers: { Accept: binary ? 'application/octet-stream' : 'application/json' } });
    if (!r.ok) {
      const e = new Error(`herald ${path}: HTTP ${r.status}`);
      Object.assign(e, { status: r.status, url });
      throw e;
    }
    return binary ? new Uint8Array(await r.arrayBuffer()) : r.json();
  }

  season() { return this.get(heraldPaths.season()); }
  async overview(ring, bell = 'latest') { return decodeOverview(await this.get(heraldPaths.overview(ring, bell), { binary: true })); }
  async province(p, q, bell = 'latest') { return decodeEnvelope(await this.get(heraldPaths.province(p, q, bell))); }
  clash(p, q, bell) { return this.get(heraldPaths.clash(p, q, bell)); }
  bell(bell, region) { return this.get(heraldPaths.bell(bell, region)); }
  me(wallet) { return this.get(heraldPaths.me(wallet)); }
  events(after) { return this.get(heraldPaths.events(after)); }
}
