import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createStateStore, DEFAULTS, LOCAL_DIR, loadConfig, OPTIONS, parseArgs } from '../src/config.mjs';

test('parseArgs: flags with values, bare flags, positional words, defaults', () => {
  const a = parseArgs(['node', 'x.mjs', '--new-season', '--port', '4200', '--tick-seconds', '5', 'extra', '--market', 'off', '--verbose'], { port: '1' });
  assert.equal(a.newSeason, true);
  assert.equal(a.port, '4200');
  assert.equal(a.tickSeconds, '5');
  assert.equal(a.market, 'off');
  assert.equal(a.verbose, true);
  assert.deepEqual(a._, ['node', 'x.mjs', 'extra']);
  assert.equal(parseArgs([], { port: '1' }).port, '1');
});

test('loadConfig: flag over environment over defaults; same env names as before', () => {
  const cfg = loadConfig({ argv: [], env: {} });
  assert.equal(cfg.port, 4191);
  assert.equal(cfg.baseRpc, DEFAULTS.baseRpc);
  assert.equal(cfg.entryFee, 10_000_000n);
  assert.equal(cfg.market, true);
  assert.equal(cfg.newSeason, false);
  assert.equal(cfg.stateFile, path.join(LOCAL_DIR, 'season.json'));
  const env = { PS_PORT: '4300', PS_MARKET: 'off', PS_ENTRY_FEE: '5', PS_STATE: 'b.json', PS_TICK_SECONDS: '12' };
  const fromEnv = loadConfig({ argv: [], env });
  assert.deepEqual([fromEnv.port, fromEnv.market, fromEnv.entryFee, fromEnv.tickSeconds, path.basename(fromEnv.stateFile)], [4300, false, 5n, 12, 'b.json']);
  const flags = loadConfig({ argv: ['--port', '4400', '--state', 'c.json', '--new-season'], env });
  assert.deepEqual([flags.port, path.basename(flags.stateFile), flags.newSeason], [4400, 'c.json', true]);
  assert.equal(path.basename(loadConfig({ argv: [], env: {}, defaults: { stateFile: 'e2e.json' } }).stateFile), 'e2e.json');
  assert.throws(() => loadConfig({ argv: ['--port', 'abc'], env: {} }), /not a number/);
  assert.deepEqual(OPTIONS.map(o => o.env).filter(e => !e.startsWith('PS_')), []);
  assert.notEqual(DEFAULTS.port, 4190, 'never the fetch-blocked port');
});

test('state stores write only their own file', () => {
  const dir = mkdtempSync(path.join(os.tmpdir(), 'state-'));
  const a = createStateStore(path.join(dir, 'a.json'));
  const b = createStateStore(path.join(dir, 'nested', 'b.json'));
  assert.equal(a.load(), null);
  a.save({ seasonId: '1', members: [] });
  b.save({ seasonId: '2', members: [] });
  a.state.members.push({ index: 0 });
  a.save();
  assert.deepEqual(JSON.parse(readFileSync(a.file, 'utf8')), { seasonId: '1', members: [{ index: 0 }] });
  assert.deepEqual(JSON.parse(readFileSync(b.file, 'utf8')), { seasonId: '2', members: [] });
  assert.equal(createStateStore(b.file).load().seasonId, '2');
  assert.ok(!existsSync(`${a.file}.${process.pid}.tmp`));
  assert.throws(() => createStateStore(path.join(dir, 'c.json')).save(), /no season state/);
});
