// Who holds each site: the herald's `/h/roster/{ring}/latest.bin`
// (frontier-node herald `roster.rs`): per province 12 × {citizen_tag u64,
// founded_bell u32}. The page turns a tag into a name and a face
// (identity.mjs); this module only decodes and keeps the file. A herald
// without the file (404) leaves the roster empty: holdings then show their
// faction and tier only, never a made-up owner.
//
//   const r = createRoster({ base, onChange });  r.ensure(ring);  r.ownerOf(p, q, site, bell?)
//
// Spectators fetch one small file per open ring (cached by the herald per
// fold version, `max-age=30`), at most once a minute.

export const ROSTER_MAGIC = 'PSFRS1\0\0';
export const ROSTER_HEADER = 32;
export const ROSTER_SITE = 12;
export const ROSTER_RECORD = 4 + 12 * ROSTER_SITE;
export const ROSTER_REFRESH_MS = 60_000;

/** Decode a roster file → `{season, ring, bell, provinces: Map("P,Q" → [{tag, bell} | null] × 12)}`. */
export function decodeRoster(bytes) {
  const b = bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes);
  if (b.length < ROSTER_HEADER || String.fromCharCode(...b.subarray(0, 8)) !== ROSTER_MAGIC) throw Object.assign(new Error('not a roster'), { code: 'BadRoster' });
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const n = dv.getUint16(18, true);
  if (b.length !== ROSTER_HEADER + n * ROSTER_RECORD) throw Object.assign(new Error('roster length'), { code: 'BadRoster' });
  const out = { season: dv.getBigUint64(8, true), ring: dv.getUint16(16, true), bell: dv.getUint32(20, true), provinces: new Map() };
  for (let i = 0; i < n; i++) {
    const o = ROSTER_HEADER + i * ROSTER_RECORD;
    const p = dv.getInt16(o, true), q = dv.getInt16(o + 2, true);
    const sites = [];
    for (let s = 0; s < 12; s++) {
      const at = o + 4 + s * ROSTER_SITE;
      const tag = dv.getBigUint64(at, true);
      sites.push(tag === 0n ? null : { tag, bell: dv.getUint32(at + 8, true) });
    }
    out.provinces.set(`${p},${q}`, sites);
  }
  return out;
}

/** The roster store of a page. `fetch` is injectable (tests). */
export function createRoster({ base = '', onChange = () => {}, fetch: f = (...a) => globalThis.fetch(...a), now = () => Date.now() } = {}) {
  const rings = new Map(); // ring → {at, data|null, loading}
  const sites = new Map(); // "P,Q" → sites
  const load = async ring => {
    const st = rings.get(ring);
    st.loading = true;
    try {
      const r = await f(`${base}/h/roster/${ring}/latest.bin`, { cache: 'no-cache' });
      if (r.ok) {
        const d = decodeRoster(new Uint8Array(await r.arrayBuffer()));
        st.data = d;
        for (const [k, v] of d.provinces) sites.set(k, v);
        onChange();
      }
    } catch { /* no roster: names stay hidden */ }
    st.loading = false;
    st.at = now();
  };
  return {
    /** Make sure the ring's roster is loaded (and refreshed once a minute). */
    ensure(ring) {
      if (!Number.isInteger(ring) || ring < 0) return;
      let st = rings.get(ring);
      if (!st) { st = { at: 0, data: null, loading: false }; rings.set(ring, st); }
      if (!st.loading && now() - st.at >= ROSTER_REFRESH_MS) load(ring);
    },
    /** `{tag, bell}` of the holder of (P, Q, site), or null; with `bell`, only an owner founded by then. */
    ownerOf(p, q, site, bell = null) {
      const o = sites.get(`${p},${q}`)?.[site] ?? null;
      if (!o) return null;
      return bell !== null && bell !== undefined && o.bell > bell ? null : o;
    },
    /** Seed a site from data the page has first-hand (the viewer's own Holding: `ownerCitizen`). */
    put(p, q, site, tag, bell = 0) {
      const k = `${p},${q}`;
      const list = sites.get(k) ?? Array(12).fill(null);
      list[site] = { tag, bell };
      sites.set(k, list);
    },
    size: () => sites.size,
  };
}
