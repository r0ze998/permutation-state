// The fixture world read back by the page's own modules in node, before any
// browser runs: the herald client decodes and checks every file the scenes
// use (season pins, the `me` key checks for each viewer stage, envelopes,
// overviews, the bell-region record, the clash report), and flog decodes
// every event. A world the page would refuse fails here, not as a blank
// screenshot.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { startServer } from './server.mjs';
import * as W from './world.mjs';
import { createHerald } from '../../permutation-server/web/frontier/herald.mjs';
import { setPin } from '../../permutation-server/web/frontier/fchainio.mjs';
import { decodePage } from '../../permutation-server/web/frontier/flog.mjs';
import { landState } from '../../permutation-server/web/frontier/fland.mjs';
import { toHex } from '../../permutation-server/web/sdk/bytes.mjs';
import { regionOf } from '../../permutation-server/web/frontier/fgeo.mjs';

test('the fixture herald: every file the scenes read decodes and passes the page\'s checks', async () => {
  const srv = await startServer();
  try {
    const h = createHerald({ base: srv.url });
    const s = await h.season();
    assert.ok(s.ok, `${s.code} ${s.error ?? ""}`);
    const pin = setPin({ programId: s.record.programId, cluster: s.record.cluster, seasonId: s.record.season, seasonAddress: s.record.seasonAddress, rulesetHash: toHex(s.season.rulesetHash) });
    h.pin(s.record.season, pin.addresses);
    for (let d = 0; d <= 2; d++) {
      const o = await h.overview(d);
      assert.ok(o.ok, `overview ${d}: ${o.code}`);
      assert.ok(o.provinces.length >= 1);
    }
    const expect = { none: 'none', joined: 'joined', holding: 'final' };
    for (const stage of ['none', 'joined', 'holding']) {
      srv.stage(stage);
      const me = await h.me(srv.viewer.wallet);
      assert.ok(me.ok, `${stage}: ${me.code} ${me.error ?? ''}`);
      assert.equal(landState(me.citizen).stage, expect[stage], stage);
      if (me.citizen) assert.equal(me.citizen.session.length, 32);
    }
    const env = await h.province(W.HOME.p, W.HOME.q, 'latest');
    assert.ok(env.ok, env.code);
    assert.equal(env.province.entries.filter(e => e.state >= 1).length, 3);
    assert.equal(env.province.camp.state, 1);
    const other = await h.province(3, -1, 'latest');
    assert.ok(other.ok, other.code);
    const b = await h.bellRegion(W.BELL, regionOf(W.HOME.p, W.HOME.q));
    assert.ok(b.ok && b.anchor, b.code);
    const c = await h.clash(W.HOME.p, W.HOME.q, W.REPORT_BELL);
    assert.ok(c.ok && c.inputs, c.code);
    assert.equal(c.inputs.nPresent, 2);
    const ev = await h.events(0);
    assert.ok(ev.ok);
    const page = decodePage(ev.events);
    assert.equal(page.records.length, ev.events.length, 'every fixture event decodes');
    assert.ok(page.records.some(r => r.record.name === 'CLASH' && r.record.bell === W.REPORT_BELL));
    // Only the paths the page asks for exist; nothing failed so far.
    assert.deepEqual(srv.requests.filter(r => r.code >= 400), []);
  } finally {
    await srv.close();
  }
});
