// Loader for frontier.wasm, the rules-v10 kernels (contract §9.5; web
// design §3.3). No wasm-bindgen: every export is `name(ptr, len) -> ptr`
// over linear memory, borsh in, a frame out (`status u8 | len u32 LE |
// payload`), released with `free(ptr, 5 + len)`; inputs go in buffers from
// `alloc(len)`. Status 0 = the borsh answer, 1 = bad input, 2 = the kernel
// refused ({code u8, arg u32}), 3 = not in this build.
//
// Loaded lazily, on first need (tile LOD, composer, practice, verify); the
// province-level map and every panel work without it. The file is checked
// against its published sha256 before it is instantiated, its ABI version
// and export list are checked, and its ruleset hash must be the pinned one
// (abi.mjs, from frontier-abi): practice and verification refuse to run on
// another kernel.
//
// The borsh encoders below are pinned against the Rust side by
// web-frontier-wasm.test.mjs (frontier-wasm/vectors/wasm-vectors.json).
import { RULESET_HASH } from './abi.mjs';

export const ABI_VERSION = 1;
export const OK = 0, BAD_INPUT = 1, REFUSED = 2, UNAVAILABLE = 3;
export const EXPORTS = Object.freeze([
  'abi_version', 'ruleset_hash', 'province_of', 'province_centre', 'ring_of', 'wedge_of', 'region_of', 'generate_province',
  'plan_path', 'path_cost', 'earliest_arrival_bell', 'check_arrival_bell', 'bell_at', 'bell_start', 'tlock_round', 'seed_round',
  'plaintext_pack', 'plaintext_unpack', 'plaintext_validate', 'commit', 'salt_of', 'body_xor', 'seal_root', 'ct_hash',
  'resolve_clash', 'resolve_from_inputs', 'reachable', 'accrual_at',
]);
/** UnitType in kernel order (borsh enum index). */
export const UNITS = Object.freeze(['Spearman', 'Archer', 'Horseman', 'Pikeman', 'Crossbowman', 'Knight', 'Scout', 'Settler']);
/** Terrain in kernel order. */
export const TERRAIN = Object.freeze(['Grassland', 'Plains', 'Forest', 'Hills', 'Mountain', 'Water']);

// ------------------------------------------------------------------ borsh (the subset the exports use)
const big = v => BigInt(typeof v === 'bigint' ? v : String(v));
const hexBytes = s => Uint8Array.from(String(s).match(/../g) ?? [], x => parseInt(x, 16));
const asBytes = (v, n) => {
  const b = typeof v === 'string' ? hexBytes(v) : Uint8Array.from(v);
  if (n !== undefined && b.length !== n) throw new Error(`expected ${n} bytes, got ${b.length}`);
  return b;
};

export class Writer {
  constructor() { this.parts = []; }
  put(n, fn) { const b = new Uint8Array(n); fn(new DataView(b.buffer)); this.parts.push(b); return this; }
  u8(v) { return this.put(1, d => d.setUint8(0, Number(v))); }
  bool(v) { return this.u8(v ? 1 : 0); }
  u16(v) { return this.put(2, d => d.setUint16(0, Number(v), true)); }
  i16(v) { return this.put(2, d => d.setInt16(0, Number(v), true)); }
  u32(v) { return this.put(4, d => d.setUint32(0, Number(v), true)); }
  i32(v) { return this.put(4, d => d.setInt32(0, Number(v), true)); }
  u64(v) { return this.put(8, d => d.setBigUint64(0, big(v), true)); }
  i64(v) { return this.put(8, d => d.setBigInt64(0, big(v), true)); }
  fixed(v, n) { this.parts.push(asBytes(v, n)); return this; }
  vec(items, fn) { this.u32(items.length); for (const it of items) fn(this, it); return this; }
  bytes() { const n = this.parts.reduce((a, p) => a + p.length, 0), out = new Uint8Array(n); let o = 0; for (const p of this.parts) { out.set(p, o); o += p.length; } return out; }
}

export class Reader {
  constructor(b) { this.b = Uint8Array.from(b); this.dv = new DataView(this.b.buffer); this.o = 0; }
  take(n) { if (this.o + n > this.b.length) throw new Error('borsh: out of data'); const o = this.o; this.o += n; return o; }
  u8() { return this.dv.getUint8(this.take(1)); }
  bool() { return this.u8() !== 0; }
  u16() { return this.dv.getUint16(this.take(2), true); }
  i16() { return this.dv.getInt16(this.take(2), true); }
  u32() { return this.dv.getUint32(this.take(4), true); }
  i32() { return this.dv.getInt32(this.take(4), true); }
  u64() { return this.dv.getBigUint64(this.take(8), true); }
  i64() { return this.dv.getBigInt64(this.take(8), true); }
  fixed(n) { const o = this.take(n); return this.b.slice(o, o + n); }
  vec(fn) { const n = this.u32(); return Array.from({ length: n }, () => fn(this)); }
  option(fn) { return this.u8() ? fn(this) : null; }
  end() { if (this.o !== this.b.length) throw new Error('borsh: trailing bytes'); }
}

const unitIndex = u => (typeof u === 'string' ? UNITS.indexOf(u) : Number(u));
const hex = (w, h) => w.i32(h.q).i32(h.r);
const drand = (w, d) => w.i64(d.genesis).u32(d.period);
const seeds = (w, list) => w.vec(list, (x, s) => x.u32(s.ring).fixed(s.seed, 32));
const pq = (w, a) => w.i32(a.p).i32(a.q);

/** Arguments → borsh input, per export (argument names as in frontier-wasm `api`). */
export const ENCODE = Object.freeze({
  abi_version: () => new Uint8Array(0),
  ruleset_hash: () => new Uint8Array(0),
  province_of: a => new Writer().i32(a.q).i32(a.r).bytes(),
  province_centre: a => pq(new Writer(), a).bytes(),
  ring_of: a => pq(new Writer(), a).bytes(),
  wedge_of: a => pq(new Writer(), a).bytes(),
  region_of: a => pq(new Writer(), a).bytes(),
  generate_province: a => new Writer().fixed(a.ring_seed, 32).i32(a.p).i32(a.q).bytes(),
  plan_path: a => { const w = new Writer(); hex(w, a.start); hex(w, a.dest); w.u8(unitIndex(a.unit)).vec(a.blocked ?? [], hex); return seeds(w, a.seeds ?? []).bytes(); },
  path_cost: a => { const w = new Writer(); hex(w, a.start); w.vec(a.dirs, (x, d) => x.u8(d)).u8(unitIndex(a.unit)); return seeds(w, a.seeds ?? []).bytes(); },
  earliest_arrival_bell: a => new Writer().i64(a.genesis_ts).i64(a.depart_ts).u32(a.secs).bytes(),
  check_arrival_bell: a => new Writer().i64(a.genesis_ts).i64(a.depart_ts).u32(a.secs).u32(a.chosen).bytes(),
  bell_at: a => new Writer().i64(a.genesis_ts).i64(a.t).bytes(),
  bell_start: a => new Writer().i64(a.genesis_ts).u32(a.bell).bytes(),
  tlock_round: a => { const w = new Writer(); drand(w, a.drand); return w.i64(a.genesis_ts).u32(a.bell).bytes(); },
  seed_round: a => { const w = new Writer(); drand(w, a.drand); return w.i64(a.a).u32(a.window).u32(a.margin).bytes(); },
  plaintext_pack: a => new Writer().u8(a.version).u64(a.host_id).u32(a.arrive_bell).i16(a.dest_p).i16(a.dest_q).u8(a.dest_tile).u8(a.stance)
    .u16(a.retreat_bps).u8(a.path_len).fixed(a.path, 12).fixed(a.reserved ?? new Uint8Array(3), 3).bytes(),
  plaintext_unpack: a => asBytes(a.plain, 37),
  plaintext_validate: a => new Writer().fixed(a.plain, 37).u64(a.host_id).u32(a.arrive_bell).bytes(),
  commit: a => new Writer().fixed(a.plain, 37).fixed(a.salt, 32).bytes(),
  salt_of: a => asBytes(a.k, 16),
  body_xor: a => new Writer().fixed(a.k, 16).fixed(a.plain, 37).bytes(),
  seal_root: a => new Writer().fixed(a.commit, 32).fixed(a.ct_hash, 32).bytes(),
  ct_hash: a => asBytes(a.seal, 165),
  resolve_clash: a => asBytes(a),
  resolve_from_inputs: a => asBytes(a ?? []),
  reachable: a => { const w = new Writer(); hex(w, a.origin); hex(w, a.dest); return w.i64(a.genesis_ts).i64(a.depart_ts).u32(a.target_bell).u8(unitIndex(a.unit)).bytes(); },
  accrual_at: a => new Writer().i64(a.accrual.value).i64(a.accrual.rate).i64(a.accrual.cap).i64(a.accrual.t0).i64(a.accrual.frac).i64(a.t).bytes(),
});

const readHex = r => ({ q: r.i32(), r: r.i32() });
const readPc = r => ({ p: r.i32(), q: r.i32() });
const one = fn => b => { const r = new Reader(b); const v = fn(r); r.end(); return v; };
const plainFields = r => ({ version: r.u8(), host_id: r.u64(), arrive_bell: r.u32(), dest_p: r.i16(), dest_q: r.i16(), dest_tile: r.u8(), stance: r.u8(), retreat_bps: r.u16(), path_len: r.u8(), path: r.fixed(12), reserved: r.fixed(3) });

/** Borsh answer → value, per export. */
export const DECODE = Object.freeze({
  abi_version: one(r => r.u32()),
  ruleset_hash: one(r => r.fixed(32)),
  province_of: one(r => ({ ...readPc(r), idx: r.u8() })),
  province_centre: one(readHex),
  ring_of: one(r => r.u32()),
  wedge_of: one(r => r.option(x => x.u8())),
  region_of: one(r => r.u8()),
  generate_province: one(r => ({
    terrain: {
      terrain: Array.from({ length: 61 }, () => r.u8()),
      resource: Array.from({ length: 61 }, () => r.option(x => x.u8())),
      sites: Array.from(r.fixed(12)),
      siteCount: r.u8(),
    },
    passableMask: r.u64(),
    centre: readHex(r),
    ring: r.u32(),
    wedge: r.option(x => x.u8()),
    region: r.u8(),
  })),
  plan_path: one(r => r.option(x => ({ dirs: x.vec(y => y.u8()), pathLen: x.u8(), path: x.fixed(12), secs: x.u32(), hexes: x.u32(), provinces: x.vec(readPc) }))),
  path_cost: one(r => ({ secs: r.u32(), hexes: r.u32(), provinces: r.vec(readPc), end: readHex(r) })),
  earliest_arrival_bell: one(r => r.u32()),
  check_arrival_bell: one(() => null),
  bell_at: one(r => r.option(x => x.u32())),
  bell_start: one(r => r.i64()),
  tlock_round: one(r => r.u64()),
  seed_round: one(r => r.u64()),
  plaintext_pack: one(r => r.fixed(37)),
  plaintext_unpack: one(plainFields),
  plaintext_validate: one(() => null),
  commit: one(r => r.fixed(32)),
  salt_of: one(r => r.fixed(32)),
  body_xor: one(r => r.fixed(37)),
  seal_root: one(r => r.fixed(32)),
  ct_hash: one(r => r.fixed(32)),
  resolve_clash: b => Uint8Array.from(b),
  resolve_from_inputs: b => Uint8Array.from(b),
  reachable: one(r => r.bool()),
  accrual_at: one(r => r.i64()),
});

/** A refusal payload → {code, arg}. */
export const decodeRefusal = one(r => ({ code: r.u8(), arg: r.u32() }));

// ------------------------------------------------------------------ the kernel
const toHexStr = b => Array.from(b, x => x.toString(16).padStart(2, '0')).join('');

export class WasmError extends Error {
  constructor(code, message) { super(message); this.code = code; }
}

export class Kernel {
  /** `exports`: an instance's exports (memory, alloc, free, and the functions). */
  constructor(exports) {
    for (const n of ['memory', 'alloc', 'free', ...EXPORTS]) if (!(n in exports)) throw new WasmError('MissingExport', `frontier.wasm has no ${n}`);
    this.x = exports;
  }

  /** Call an export with borsh input bytes → {status, payload}. */
  callRaw(name, input) {
    const x = this.x, n = input.length;
    const ptr = x.alloc(n);
    new Uint8Array(x.memory.buffer, ptr, n).set(input);
    const out = x[name](ptr, n);
    x.free(ptr, n);
    const head = new Uint8Array(x.memory.buffer, out, 5);
    const status = head[0], len = head[1] | (head[2] << 8) | (head[3] << 16) | (head[4] * 2 ** 24);
    const payload = new Uint8Array(x.memory.buffer, out + 5, len).slice();
    x.free(out, 5 + len);
    return { status, payload };
  }

  /**
   * Call an export with arguments → `{ok: true, value}` or `{ok: false,
   * status, code?, arg?, reason?}` (a refusal's code, a bad input's reason).
   */
  call(name, args) {
    if (!EXPORTS.includes(name)) throw new WasmError('NoExport', name);
    const { status, payload } = this.callRaw(name, ENCODE[name](args));
    if (status === OK) return { ok: true, value: DECODE[name](payload) };
    if (status === REFUSED) return { ok: false, status, ...decodeRefusal(payload) };
    return { ok: false, status, reason: new TextDecoder().decode(payload) };
  }

  /** The kernel's ruleset hash (hex). */
  rulesetHash() { return toHexStr(this.call('ruleset_hash').value); }
}

/**
 * Fetch, check and instantiate frontier.wasm: its sha256 must equal the
 * published one (`frontier.wasm.sha256`), its ABI version 1, its ruleset
 * hash the pinned one. Throws WasmError.
 */
export async function loadKernel({ url = new URL('./wasm/frontier.wasm', import.meta.url), fetch: f = (...a) => globalThis.fetch(...a), sha256Hex = null, ruleset = RULESET_HASH } = {}) {
  let want = sha256Hex;
  if (!want) {
    const r = await f(`${url}.sha256`, { cache: 'no-cache' });
    if (!r.ok) throw new WasmError('NoWasm', `frontier.wasm.sha256: HTTP ${r.status}`);
    want = (await r.text()).trim().split(/\s+/)[0];
  }
  const r = await f(url, { cache: 'force-cache' });
  if (!r.ok) throw new WasmError('NoWasm', `frontier.wasm: HTTP ${r.status}`);
  const bytes = new Uint8Array(await r.arrayBuffer());
  const got = toHexStr(new Uint8Array(await globalThis.crypto.subtle.digest('SHA-256', bytes)));
  if (got !== want) throw new WasmError('WasmHashMismatch', `frontier.wasm sha256 ${got}, published ${want}`);
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const k = new Kernel(instance.exports);
  const v = k.call('abi_version');
  if (!v.ok || v.value !== ABI_VERSION) throw new WasmError('WasmAbiMismatch', `frontier.wasm ABI ${v.value}`);
  if (k.rulesetHash() !== ruleset) throw new WasmError('RulesetMismatch', 'frontier.wasm was built from other rules');
  return k;
}

let loading = null;
/** The page's kernel, loaded once on first need (a failure is kept: the caller shows it and offers a retry). */
export function kernel(opts) {
  if (!loading) loading = loadKernel(opts).catch(e => { loading = null; throw e; });
  return loading;
}
