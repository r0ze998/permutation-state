import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { checkConfig, ConfigError, createStateStore, DEFAULTS, LOCAL_DIR, loadConfig, OPTIONS, parseArgs } from '../src/config.mjs';

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

test('loadConfig: the listeners, the registration window and the switches', () => {
  const cfg = loadConfig({ argv: [], env: {} });
  assert.deepEqual([cfg.port, cfg.publicPort, cfg.publicHost, cfg.trustProxy, cfg.minCrankSol], [4191, 4194, '127.0.0.1', true, 0.3]);
  assert.deepEqual([cfg.publicBaseRpc, cfg.publicErRpc], [null, null], 'no RPC URL published unless given');
  assert.deepEqual([cfg.registrationSeconds, cfg.waitExternal, cfg.allowIdentifiableAi, cfg.devWallet, cfg.ai, cfg.deposit], [600, 0, false, false, 2, 0n]);
  const on = loadConfig({ argv: ['--trust-proxy', '--public-port', '0', '--registration-seconds', '0', '--allow-identifiable-ai', '--dev-wallet'], env: {} });
  assert.deepEqual([on.trustProxy, on.publicPort, on.registrationSeconds, on.allowIdentifiableAi, on.devWallet], [true, 0, 0, true, true]);
  assert.equal(loadConfig({ argv: [], env: { PS_TRUST_PROXY: '1', PS_DEV_WALLET: 'true' } }).devWallet, true);
  assert.throws(() => loadConfig({ argv: ['--registration-seconds', '-5'], env: {} }), /non-negative/);
  assert.equal(OPTIONS.some(o => o.key === 'humans'), false);
});

test('--humans / PS_HUMANS were removed: a clear error, not a silent default', () => {
  assert.throws(() => loadConfig({ argv: ['--humans', '1'], env: {} }), e => e instanceof ConfigError && /people join with their own wallets/.test(e.message));
  assert.throws(() => loadConfig({ argv: [], env: { PS_HUMANS: '0' } }), ConfigError);
});

test('checkConfig: dev mode shows who the AI members are (refused without --allow-identifiable-ai); the dev wallet is localnet only', () => {
  const cfg = over => ({ ...loadConfig({ argv: [], env: {} }), ...over });
  assert.throws(() => checkConfig(cfg({ registrationSeconds: 0 }), { creating: true }), e => e instanceof ConfigError && /--allow-identifiable-ai/.test(e.message));
  assert.deepEqual(checkConfig(cfg({ registrationSeconds: 0, allowIdentifiableAi: true }), { creating: true }), []);
  assert.deepEqual(checkConfig(cfg({ registrationSeconds: 0, ai: 0 }), { creating: true }), [], 'no AI members: nothing to show');
  assert.deepEqual(checkConfig(cfg({ registrationSeconds: 0 }), { creating: false }), [], 'resuming a season: its own settings');
  assert.match(checkConfig(cfg({ waitExternal: 2 }), { creating: true })[0], /--wait-external 2 is ignored/);
  assert.throws(() => checkConfig(cfg({ devWallet: true, cluster: 'devnet' })), /localnet only/);
  assert.deepEqual(checkConfig(cfg({ devWallet: true })), []);
  assert.throws(() => checkConfig(cfg({ publicPort: 4191 })), /operator port/);
  assert.throws(() => checkConfig(cfg({ publicPort: 4190 })), /4190/);
});

test('the public port follows the operator port (+3, 4191 → 4194), so gateways side by side do not collide; never 4190', () => {
  assert.equal(loadConfig({ argv: [], env: {} }).publicPort, 4194);
  assert.equal(loadConfig({ argv: ['--port', '4192'], env: {} }).publicPort, 4195, 'a second gateway with --port alone');
  assert.equal(loadConfig({ argv: [], env: { PS_PORT: '4300' } }).publicPort, 4303);
  assert.equal(loadConfig({ argv: ['--port', '4192', '--public-port', '0'], env: {} }).publicPort, 0, 'explicit: none');
  assert.equal(loadConfig({ argv: ['--port', '4192'], env: { PS_PUBLIC_PORT: '4400' } }).publicPort, 4400);
  assert.equal(loadConfig({ argv: [], env: {}, defaults: { publicPort: 5000 } }).publicPort, 5000);
  const derived = loadConfig({ argv: ['--port', '4187'], env: {} });
  assert.equal(derived.publicPort, 4190);
  assert.throws(() => checkConfig(derived), e => e instanceof ConfigError && /--port 4187 puts the public listener on 4190/.test(e.message) && /--public-port/.test(e.message));
  assert.deepEqual(checkConfig(loadConfig({ argv: ['--port', '4187', '--public-port', '4188'], env: {} })), []);
});

test('--trust-proxy is on by default; --no-trust-proxy or PS_TRUST_PROXY=0 turns it off; the public RPC URLs are opt-in', () => {
  assert.equal(loadConfig({ argv: [], env: {} }).trustProxy, true);
  assert.equal(loadConfig({ argv: ['--no-trust-proxy'], env: {} }).trustProxy, false);
  assert.equal(loadConfig({ argv: ['--no-trust-proxy', '--port', '4200'], env: { PS_TRUST_PROXY: '1' } }).trustProxy, false, 'the flag wins');
  assert.equal(loadConfig({ argv: [], env: { PS_TRUST_PROXY: '0' } }).trustProxy, false);
  assert.equal(loadConfig({ argv: ['--trust-proxy', 'off'], env: {} }).trustProxy, false);
  const pub = loadConfig({ argv: ['--public-base-rpc', 'https://api.devnet.solana.com'], env: { PS_PUBLIC_ER_RPC: 'https://devnet.magicblock.app' } });
  assert.deepEqual([pub.publicBaseRpc, pub.publicErRpc], ['https://api.devnet.solana.com', 'https://devnet.magicblock.app']);
});
