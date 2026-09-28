// The herald files recorded from a local season (W6-D; web design §13.1):
// `test/fixtures/frontier/recorded/` was written by the scripted onboarding
// run (screens/live/onboarding.live.mjs with LIVE_RECORD) on a
// `frontier-stack` localnet season on the test beacon. Unlike the
// synthetic fixtures (make-fixtures.mjs), nothing here was built by hand:
// the page's own herald client must accept every file, and "verify in this
// browser" must recompute the recorded clash of the run's first march — a
// real camp clash resolved by the program — to the Province's digest with
// the committed frontier.wasm.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { createHerald } from '../../permutation-server/web/frontier/herald.mjs';
import { setPin } from '../../permutation-server/web/frontier/fchainio.mjs';
import { seasonClock } from '../../permutation-server/web/frontier/clock.mjs';
import { decodePage } from '../../permutation-server/web/frontier/flog.mjs';
import { landState } from '../../permutation-server/web/frontier/fland.mjs';
import { regionOf } from '../../permutation-server/web/frontier/fgeo.mjs';
import { verifyClash } from '../../permutation-server/web/frontier/screens/report.mjs';
import { loadKernel } from '../../permutation-server/web/frontier/wasm.mjs';
import { toHex } from '../../permutation-server/web/sdk/bytes.mjs';

const DIR = fileURLToPath(new URL('fixtures/frontier/recorded/', import.meta.url));
const WASM = fileURLToPath(new URL('../../permutation-server/web/frontier/wasm/frontier.wasm', import.meta.url));
const M = JSON.parse(readFileSync(`${DIR}manifest.json`, 'utf8'));

/** A fetch over the recorded files, by the herald path each was fetched from. */
function recordedFetch() {
  const byPath = new Map(Object.entries(M.files).map(([name, f]) => [f.path, name]));
  return async url => {
    const u = new URL(url);
    const name = byPath.get(`${u.pathname}${u.search}`);
    if (!name) return new Response('not recorded', { status: 404 });
    return new Response(readFileSync(`${DIR}${name}`), { status: 200 });
  };
}

test('the recording: every file listed, byte-identical to its manifest hash, from a localnet season on the test beacon', () => {
  assert.equal(M.format, 'frontier-web-recorded-v1');
  assert.ok(Object.keys(M.files).length >= 8);
  for (const [name, f] of Object.entries(M.files)) {
    assert.ok(existsSync(`${DIR}${name}`), name);
    assert.equal(createHash('sha256').update(readFileSync(`${DIR}${name}`)).digest('hex'), f.sha256, name);
  }
  const season = JSON.parse(readFileSync(`${DIR}season.json`, 'utf8'));
  assert.equal(season.cluster, 'localnet');
  assert.equal(M.verify, 'match', 'the browser run verified this clash');
});

test('the page\'s herald client accepts every recorded file (season pins, overviews, me, envelopes, bell record, events)', async () => {
  const h = createHerald({ base: 'http://herald.test', fetch: recordedFetch() });
  const s = await h.season();
  assert.ok(s.ok, `${s.code} ${s.error ?? ''}`);
  const pin = setPin({ programId: s.record.programId, cluster: s.record.cluster, seasonId: s.record.season, seasonAddress: s.record.seasonAddress, rulesetHash: toHex(s.season.rulesetHash) });
  h.pin(s.record.season, pin.addresses);
  for (const r of s.record.rings) {
    const o = await h.overview(r.d);
    assert.ok(o.ok, `overview ${r.d}: ${o.code}`);
  }
  const me = await h.me(M.wallet);
  assert.ok(me.ok, `me: ${me.code} ${me.error ?? ''}`);
  assert.equal(landState(me.citizen).stage, 'final', 'the onboarding wallet holds a final holding');
  const { p, q, bell } = M.clash;
  for (const b of [bell, 'latest']) {
    const env = await h.province(p, q, b);
    assert.ok(env.ok, `province ${p},${q}/${b}: ${env.code}`);
  }
  const br = await h.bellRegion(bell, regionOf(p, q));
  assert.ok(br.ok, br.code);
  const ev = await h.events(0);
  assert.ok(ev.ok, ev.code);
  const page = decodePage(ev.events);
  assert.ok(page.records.length > 0, 'events recorded');
  assert.equal(page.bad, 0, 'every recorded event decodes');
});

test('verify in this browser on the recorded clash: the committed frontier.wasm rebuilds it from the account bytes and matches the Province\'s digest', async () => {
  const h = createHerald({ base: 'http://herald.test', fetch: recordedFetch() });
  const s = await h.season();
  const pin = setPin({ programId: s.record.programId, cluster: s.record.cluster, seasonId: s.record.season, seasonAddress: s.record.seasonAddress, rulesetHash: toHex(s.season.rulesetHash) });
  h.pin(s.record.season, pin.addresses);
  const bytes = readFileSync(WASM);
  const sha = readFileSync(`${WASM}.sha256`, 'utf8').trim().split(/\s+/)[0];
  const k = await loadKernel({ url: 'x/frontier.wasm', sha256Hex: sha, fetch: async () => ({ ok: true, arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.length) }) });
  const { p, q, bell } = M.clash;
  const clash = await h.clash(p, q, bell);
  assert.ok(clash.ok, clash.code);
  const v = await verifyClash({ p, q, bell, clash, herald: h, kernel: k, clock: seasonClock(s.season), season: s.season });
  assert.equal(v.result, 'match', JSON.stringify(Object.fromEntries(Object.entries(v.steps).map(([id, x]) => [id, x.ok === true ? true : `${x.ok} ${x.why ?? ''}`]))));
  assert.equal(v.builder, 'kernel');
  assert.equal(v.steps.digest.detail, 'province', 'the Province\'s own digest after the resolve is the reference');
  assert.ok(v.outcome.fighters.some(f => f.arrival), 'the march is in the outcome');
});
