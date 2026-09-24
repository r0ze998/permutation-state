// Minimal borsh writer/reader matching the Rust `borsh` 1.x layout:
// little-endian integers, u32 length prefixes for Vec/String, 1-byte enum
// and Option tags, fixed arrays without a prefix.

export class Writer {
  constructor() { this.parts = []; }
  u8(v) { this.parts.push(Uint8Array.of(v & 0xff)); return this; }
  bool(v) { return this.u8(v ? 1 : 0); }
  u16(v) { const b = new Uint8Array(2); new DataView(b.buffer).setUint16(0, v, true); this.parts.push(b); return this; }
  u32(v) { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, v, true); this.parts.push(b); return this; }
  i32(v) { const b = new Uint8Array(4); new DataView(b.buffer).setInt32(0, v, true); this.parts.push(b); return this; }
  u64(v) { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(v), true); this.parts.push(b); return this; }
  fixed(bytes, len) {
    const b = Uint8Array.from(bytes);
    if (b.length !== len) throw new Error(`expected ${len} bytes, got ${b.length}`);
    this.parts.push(b); return this;
  }
  bytes(bytes) { const b = Uint8Array.from(bytes); this.u32(b.length); this.parts.push(b); return this; }
  string(s) { return this.bytes(new TextEncoder().encode(s)); }
  vec(items, write) { this.u32(items.length); for (const it of items) write(this, it); return this; }
  option(v, write) { if (v === null || v === undefined) return this.u8(0); this.u8(1); write(this, v); return this; }
  toBytes() {
    const n = this.parts.reduce((a, p) => a + p.length, 0);
    const out = new Uint8Array(n); let o = 0;
    for (const p of this.parts) { out.set(p, o); o += p.length; }
    return out;
  }
}

export class Reader {
  constructor(bytes, offset = 0) { this.b = Uint8Array.from(bytes); this.o = offset; this.dv = new DataView(this.b.buffer); }
  need(n) { if (this.o + n > this.b.length) throw new Error('borsh: out of data'); }
  u8() { this.need(1); return this.b[this.o++]; }
  bool() { return this.u8() !== 0; }
  u16() { this.need(2); const v = this.dv.getUint16(this.o, true); this.o += 2; return v; }
  u32() { this.need(4); const v = this.dv.getUint32(this.o, true); this.o += 4; return v; }
  i32() { this.need(4); const v = this.dv.getInt32(this.o, true); this.o += 4; return v; }
  u64() { this.need(8); const v = this.dv.getBigUint64(this.o, true); this.o += 8; return v; }
  i64() { this.need(8); const v = this.dv.getBigInt64(this.o, true); this.o += 8; return v; }
  fixed(n) { this.need(n); const v = this.b.slice(this.o, this.o + n); this.o += n; return v; }
  bytes() { return this.fixed(this.u32()); }
  string() { return new TextDecoder().decode(this.bytes()); }
  vec(read) { const n = this.u32(); const out = []; for (let i = 0; i < n; i++) out.push(read(this)); return out; }
  option(read) { return this.u8() ? read(this) : null; }
}
