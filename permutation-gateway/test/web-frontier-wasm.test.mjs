// The WASM kernel's loader (permutation-server/web/frontier/wasm.mjs)
// against frontier-wasm/vectors/wasm-vectors.json, which the frontier-wasm
// host tests record by calling every export through the same frame
// protocol the browser uses:
//  - the JS borsh encoders produce exactly the recorded input bytes, and the
//    decoders read every recorded answer;
//  - the page's JS transcriptions (fgeo, seal, clock) give the answers the
//    kernel recorded (so the no-WASM paths agree with the kernel);
//  - the loader's frame handling over real WebAssembly.Memory (a stand-in
//    instance answering from the vectors);
//  - once scripts/build-wasm.sh has produced web/frontier/wasm/frontier.wasm
//    (wasm32 target: PENDING-OWNER, O-M1-12): its hash matches the
//    published sha256, every recorded call gives the recorded answer, the
//    ruleset hash is the pinned one, and it is within 400 KB raw / 150 KB
//    gzip (contract §9.5).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';
import * as W from '../../permutation-server/web/frontier/wasm.mjs';
import * as geo from '../../permutation-server/web/frontier/fgeo.mjs';
import * as seal from '../../permutation-server/web/frontier/seal.mjs';
import * as clock from '../../permutation-server/web/frontier/clock.mjs';
import { RULESET_HASH } from '../../permutation-server/web/frontier/abi.mjs';

const REPO = new URL('../../', import.meta.url);
const V = JSON.parse(readFileSync(new URL('frontier-wasm/vectors/wasm-vectors.json', REPO), 'utf8'));
const WASM = new URL('permutation-server/web/frontier/wasm/frontier.wasm', REPO);
const hex = h => Uint8Array.from(Buffer.from(h, 'hex'));
const toHex = b => Buffer.from(b).toString('hex');
const calls = name => V.calls.filter(c => c.export === name);

test('the vector file covers every export and the pinned ruleset hash', () => {
  assert.equal(V.abi_version, W.ABI_VERSION);
  assert.equal(V.ruleset_hash, RULESET_HASH, 'frontier-wasm and frontier-abi agree on the rules');
  for (const name of W.EXPORTS) assert.ok(calls(name).length > 0, `no vector for ${name}`);
});

test('the JS encoders give the recorded borsh input for every call; the decoders read every answer', () => {
  let n = 0;
  for (const c of V.calls) {
    if (c.args !== null) {
      assert.equal(toHex(W.ENCODE[c.export](c.args)), c.input, `${c.export} ${JSON.stringify(c.args).slice(0, 80)}`);
      n++;
    }
    if (c.status === W.OK) assert.doesNotThrow(() => W.DECODE[c.export](hex(c.output)), c.export);
    if (c.status === W.REFUSED) assert.doesNotThrow(() => W.decodeRefusal(hex(c.output)));
  }
  assert.ok(n >= V.calls.length - 1);
});

test('the page\'s JS transcriptions agree with the kernel\'s recorded answers', () => {
  const ans = c => W.DECODE[c.export](hex(c.output));
  for (const c of calls('province_of')) { const l = geo.locate(c.args.q, c.args.r); assert.deepEqual(ans(c), { p: l.p, q: l.q, idx: l.idx }); }
  for (const c of calls('province_centre')) assert.deepEqual(ans(c), geo.provinceCentre(c.args.p, c.args.q));
  for (const c of calls('ring_of')) assert.equal(ans(c), geo.ringOf(c.args.p, c.args.q));
  for (const c of calls('wedge_of')) assert.equal(ans(c), geo.wedgeOf(c.args.p, c.args.q));
  for (const c of calls('region_of')) assert.equal(ans(c), geo.regionOf(c.args.p, c.args.q));
  for (const c of calls('bell_at')) assert.equal(ans(c), clock.bellAt(c.args.genesis_ts, c.args.t));
  for (const c of calls('bell_start')) assert.equal(Number(ans(c)), clock.bellStart(c.args.genesis_ts, c.args.bell));
  for (const c of calls('tlock_round')) assert.equal(Number(ans(c)), clock.tlockRound(c.args.drand, c.args.genesis_ts, c.args.bell));
  for (const c of calls('seed_round')) assert.equal(Number(ans(c)), clock.seedRound(c.args.drand, c.args.a + c.args.window, c.args.margin));
  for (const c of calls('commit')) assert.equal(toHex(ans(c)), toHex(seal.commit(hex(c.args.plain), hex(c.args.salt))));
  for (const c of calls('salt_of')) assert.equal(toHex(ans(c)), toHex(seal.saltOf(hex(c.args.k))));
  for (const c of calls('body_xor')) assert.equal(toHex(ans(c)), toHex(seal.bodyXor(hex(c.args.k), hex(c.args.plain))));
  for (const c of calls('ct_hash')) assert.equal(toHex(ans(c)), toHex(seal.ctHash(hex(c.args.seal))));
  for (const c of calls('seal_root')) assert.equal(toHex(ans(c)), toHex(seal.sealRoot(hex(c.args.commit), hex(c.args.ct_hash))));
  for (const c of calls('plaintext_pack')) {
    const a = c.args;
    assert.equal(toHex(ans(c)), toHex(seal.pack({ version: a.version, hostId: BigInt(a.host_id), arriveBell: a.arrive_bell, destP: a.dest_p, destQ: a.dest_q, destTile: a.dest_tile, stance: a.stance, retreatBps: a.retreat_bps, pathLen: a.path_len, path: hex(a.path), reserved: hex(a.reserved) })));
  }
  for (const c of calls('plaintext_validate')) {
    const why = seal.validate(seal.unpack(hex(c.args.plain)), BigInt(c.args.host_id), c.args.arrive_bell);
    assert.equal(why === null, c.status === W.OK, JSON.stringify(c.args).slice(0, 60));
  }
  const refusal = calls('path_cost').find(c => c.status === W.REFUSED);
  assert.deepEqual(W.decodeRefusal(hex(refusal.output)), { code: 9, arg: 1 }, 'a direction ≥ 6 at step 1');
  const gen = W.DECODE.generate_province(hex(calls('generate_province')[0].output));
  assert.equal(gen.terrain.terrain.length, 61);
  assert.equal(gen.ring, 2);
  assert.deepEqual(gen.centre, geo.provinceCentre(2, 0));
  assert.equal(gen.region, geo.regionOf(2, 0));
  for (let i = 0; i < 61; i++) assert.equal(Boolean((gen.passableMask >> BigInt(i)) & 1n), gen.terrain.terrain[i] < 4, `tile ${i}`);
});

/** A stand-in instance: real linear memory, a bump allocator, exports answering from the vectors. */
function fakeExports() {
  const memory = new WebAssembly.Memory({ initial: 4 });
  let top = 1024, live = 0;
  const alloc = n => { const p = top; top += n + 8; live++; return p; };
  const free = () => { live--; };
  const answers = new Map(V.calls.map(c => [`${c.export}:${c.input}`, c]));
  const x = { memory, alloc, free, live: () => live };
  for (const name of W.EXPORTS) {
    x[name] = (ptr, len) => {
      const input = toHex(new Uint8Array(memory.buffer, ptr, len));
      const c = answers.get(`${name}:${input}`);
      const payload = c ? hex(c.output) : new TextEncoder().encode('no vector');
      const out = alloc(5 + payload.length), view = new Uint8Array(memory.buffer, out, 5 + payload.length);
      view[0] = c ? c.status : W.BAD_INPUT;
      new DataView(memory.buffer).setUint32(out + 1, payload.length, true);
      view.set(payload, 5);
      return out;
    };
  }
  return x;
}

test('the loader frames calls over linear memory and releases every buffer', () => {
  const x = fakeExports();
  const k = new W.Kernel(x);
  assert.equal(k.rulesetHash(), RULESET_HASH);
  for (const c of V.calls.filter(v => v.args !== null)) {
    const r = k.call(c.export, c.args);
    assert.equal(r.ok, c.status === W.OK, c.export);
    if (r.ok) assert.deepEqual(r.value, W.DECODE[c.export](hex(c.output)));
    if (c.status === W.REFUSED) assert.deepEqual([r.code, r.arg], Object.values(W.decodeRefusal(hex(c.output))));
    if (c.status === W.UNAVAILABLE) assert.match(r.reason, /W4-A/);
  }
  assert.equal(x.live(), 0, 'every alloc freed');
  assert.throws(() => new W.Kernel({ memory: x.memory }), e => e.code === 'MissingExport');
  assert.throws(() => k.call('nope', {}), e => e.code === 'NoExport');
  const planned = k.call('plan_path', calls('plan_path')[0].args);
  assert.ok(planned.ok && planned.value && planned.value.dirs.length === planned.value.hexes);
});

test('loadKernel refuses a file whose hash is not the published one', async () => {
  const bytes = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]);
  const f = async url => ({ ok: true, status: 200, arrayBuffer: async () => bytes.buffer, text: async () => '00'.repeat(32) });
  await assert.rejects(W.loadKernel({ url: 'x/frontier.wasm', fetch: f }), e => e.code === 'WasmHashMismatch');
  await assert.rejects(W.loadKernel({ url: 'x/frontier.wasm', fetch: async () => ({ ok: false, status: 404 }) }), e => e.code === 'NoWasm');
});

test('frontier.wasm: hash, recorded answers, ruleset hash and size budget', async t => {
  if (!existsSync(WASM)) {
    t.skip('PENDING-OWNER: frontier.wasm is not built (the wasm32-unknown-unknown target waits for O-M1-12); the exports are tested on the host (frontier-wasm tests/exports.rs)');
    return;
  }
  const bytes = readFileSync(WASM);
  const published = readFileSync(new URL('frontier.wasm.sha256', WASM), 'utf8').trim().split(/\s+/)[0];
  assert.equal(createHash('sha256').update(bytes).digest('hex'), published, 'frontier.wasm matches frontier.wasm.sha256');
  assert.ok(bytes.length <= 400 * 1024, `frontier.wasm is ${bytes.length} B (budget 400 KB raw)`);
  assert.ok(gzipSync(bytes, { level: 9 }).length <= 150 * 1024, 'budget 150 KB gzip');
  const k = await W.loadKernel({ url: WASM.href, sha256Hex: published, fetch: async () => ({ ok: true, arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.length) }) });
  for (const c of V.calls) {
    const r = k.callRaw(c.export, hex(c.input));
    assert.equal(r.status, c.status, c.export);
    assert.equal(toHex(r.payload), c.output, c.export);
  }
});
