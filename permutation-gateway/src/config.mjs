// Gateway configuration: one table of defaults, one argument parser, the
// gateway's options (flag, environment variable, default), local key
// storage and the season state file.
//
// Keys live under .local/keys (git-ignored). They are disposable
// localnet/devnet keys; the gateway never creates or funds mainnet keys.
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { DEFAULT_GATEWAY, DEFAULT_SERVER } from '../client/src/http.mjs';
import { loadOrCreateKeypairSync } from '../client/src/keys.mjs';

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const LOCAL_DIR = path.join(ROOT, '.local');
export const KEYS_DIR = path.join(LOCAL_DIR, 'keys');

// The local stack (scripts/local-stack.mjs), on its own ports so it never
// touches another local validator.
const BASE_PORT = 18899;
const ER_PORT = 17799;
const ROUTER_PORT = 16699;

/** Every default of the package's tools in one place. */
export const DEFAULTS = Object.freeze({
  cluster: 'localnet',
  basePort: BASE_PORT,
  erPort: ER_PORT,
  routerPort: ROUTER_PORT,
  baseRpc: `http://127.0.0.1:${BASE_PORT}`,
  erRpc: `http://127.0.0.1:${ER_PORT}`,
  programId: 'J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n',
  erValidator: 'mAGicPQYBMvcYveUZA5F5UNNwyHvfYh5xkLS2Fr1mev',
  // Game server and gateway (never 4190: a "bad port" for browsers and fetch).
  serverUrl: DEFAULT_SERVER,
  gatewayUrl: DEFAULT_GATEWAY,
  port: Number(new URL(DEFAULT_GATEWAY).port),
  tickSeconds: 30,
  // Periodic ER→base commits (0 = none; the final undelegation always
  // commits). 180 ticks / 20 = 9, within the ER's 10 sponsored commits per
  // account. Each round goes out in small `CommitPart` intents.
  commitEvery: 20,
  entryFee: 10_000_000n, // 10 USDC (6 decimals)
  market: 'on',
  humans: 1,
  ai: 2,
  waitExternal: 0,
  registrationSeconds: 0,
  stateFile: 'season.json',
  // scripts/rpc-proxy.mjs
  proxyPort: 18999,
  proxyTarget: 'https://api.devnet.solana.com',
});

/**
 * `--some-flag value` → `{ someFlag: 'value' }`; a flag followed by another
 * flag (or nothing) is `true`; other words are collected in `_`. Accepts a
 * whole `process.argv` (the node binary and script path end up in `_`).
 */
export function parseArgs(argv = process.argv.slice(2), defaults = {}) {
  const out = { ...defaults, _: [] };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith('--')) { out._.push(a); continue; }
    const key = a.slice(2).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
    const next = argv[i + 1];
    if (next === undefined || next.startsWith('--')) out[key] = true;
    else { out[key] = next; i++; }
  }
  return out;
}

const int = v => {
  const n = Number(v);
  if (!Number.isFinite(n)) throw new Error(`not a number: ${v}`);
  return n;
};
const onOff = v => v !== 'off' && v !== false && v !== 'false';

/** The gateway's options: config key, flag, environment variable, parser. Defaults come from DEFAULTS. */
export const OPTIONS = Object.freeze([
  ['cluster', '--cluster', 'PS_CLUSTER', String],
  ['baseRpc', '--base', 'PS_BASE', String],
  ['erRpc', '--er', 'PS_ER', String],
  ['programId', '--program', 'PS_PROGRAM', String],
  ['port', '--port', 'PS_PORT', int],
  ['tickSeconds', '--tick-seconds', 'PS_TICK_SECONDS', int],
  ['commitEvery', '--commit-every', 'PS_COMMIT_EVERY', int],
  ['erValidator', '--er-validator', 'PS_ER_VALIDATOR', String],
  ['entryFee', '--entry-fee', 'PS_ENTRY_FEE', BigInt],
  // The USDC market (V5 §7.5) can be switched off per season.
  ['market', '--market', 'PS_MARKET', onOff],
  // Hosted members: claimable human members, and AI members per nation.
  ['humans', '--humans', 'PS_HUMANS', int],
  ['ai', '--ai', 'PS_AI', int],
  // Registration closes once this many outside members joined (x402), or
  // after `registrationSeconds` if that is set (0 = no time limit).
  ['waitExternal', '--wait-external', 'PS_WAIT_EXTERNAL', int],
  ['registrationSeconds', '--registration-seconds', 'PS_REGISTRATION_SECONDS', int],
  // Several gateways (one per season) can share the stack and the keys.
  ['stateFile', '--state', 'PS_STATE', String],
].map(([key, flag, env, parse]) => Object.freeze({ key, flag, env, parse })));

const flagKey = flag => flag.slice(2).replace(/-([a-z])/g, (_, c) => c.toUpperCase());

/**
 * The gateway configuration: each option from its flag, else its
 * environment variable, else `defaults` (over DEFAULTS). The state file
 * resolves against .local/. `--new-season` sets `newSeason`.
 */
export function loadConfig({ argv = process.argv.slice(2), env = process.env, defaults = {} } = {}) {
  const args = parseArgs(argv);
  const d = { ...DEFAULTS, ...defaults };
  const cfg = {};
  for (const { key, flag, env: name, parse } of OPTIONS) {
    const raw = args[flagKey(flag)] ?? env[name];
    cfg[key] = parse(raw === undefined ? d[key] : raw);
  }
  cfg.stateFile = path.resolve(LOCAL_DIR, cfg.stateFile);
  cfg.newSeason = args.newSeason === true;
  return cfg;
}

/** A named keypair persisted under .local/keys/<name>.json (created on first use). */
export function namedKey(name, dir = KEYS_DIR) {
  return loadOrCreateKeypairSync(path.join(dir, `${name}.json`));
}

/**
 * The season state (JSON) of one gateway, in one file. Every writer gets the
 * store explicitly, so a gateway (or script) with `--state` never writes
 * another season's file. `state` is the live object shared by the server,
 * the crank and the routes; `save()` writes it atomically.
 */
export function createStateStore(file) {
  return {
    file,
    state: null,
    /** Read the file (null if there is none). */
    load() {
      this.state = existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')) : null;
      return this.state;
    },
    /** Write `state` (and make it the live state). */
    save(state = this.state) {
      if (!state) throw new Error('no season state to save');
      this.state = state;
      mkdirSync(path.dirname(file), { recursive: true });
      const tmp = `${file}.${process.pid}.tmp`;
      writeFileSync(tmp, JSON.stringify(state, null, 2));
      renameSync(tmp, file);
      return state;
    },
  };
}
