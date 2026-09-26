// Gateway configuration: one table of defaults, one argument parser, the
// gateway's options (flag, environment variable, default), local key
// storage and the season state file.
//
// Keys live under .local/keys (git-ignored). They are disposable
// localnet/devnet keys; the gateway never creates or funds mainnet keys.
import { randomBytes } from 'node:crypto';
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
  // `port` is the operator listener (loopback only, every route); the public
  // listener serves browsers and agents the routes in app.mjs PUBLIC_ROUTES.
  serverUrl: DEFAULT_SERVER,
  gatewayUrl: DEFAULT_GATEWAY,
  port: Number(new URL(DEFAULT_GATEWAY).port),
  // null: the operator port + PUBLIC_PORT_OFFSET (4191 → 4194), so gateways
  // run side by side with `--port` alone do not collide; 0 = no public listener.
  publicPort: null,
  publicHost: '127.0.0.1',
  // X-Forwarded-For names the client, but only when the request comes from
  // this machine (the play server's /gw, a local reverse proxy or tunnel
  // that sets it; app.mjs clientIp). Off (`--no-trust-proxy`) only when a
  // raw TCP forwarder ends on loopback and passes the header through.
  trustProxy: true,
  // What the public listener says the RPC endpoints are (/season endpoints,
  // GET /relay endpoint): nothing unless given, since the operator's own
  // URLs may carry an API key. The operator listener shows its own.
  publicBaseRpc: null,
  publicErRpc: null,
  // Public co-signing stops (503 OperatorLowFunds) below this crank balance.
  minCrankSol: 0.3,
  tickSeconds: 30,
  // Periodic ER→base commits (0 = none; the final undelegation always
  // commits). 180 ticks / 20 = 9, within the ER's 10 sponsored commits per
  // account. Each round goes out in small `CommitPart` intents.
  commitEvery: 20,
  entryFee: 10_000_000n, // 10 USDC (6 decimals)
  market: 'on',
  // Operator AI members per nation (V5 §18).
  ai: 2,
  // Operator AI members (V5 §18): bounty per AI (USDC base units) and the
  // bond (null: AI members × entry fee × 2), escrowed at creation.
  bounty: 5_000_000n,
  bond: null,
  // The public default deposit into the nation's treasury (market,
  // contracts), refunded at the end: every member, AI or not, is offered it
  // (/season.registration.deposit).
  deposit: 0n,
  // Registration is open this long; the season starts at its end however
  // many people joined. 0 = dev mode: the AI members register at creation
  // and the season starts once `waitExternal` other members joined (it
  // shows who the AI members are: --allow-identifiable-ai).
  registrationSeconds: 600,
  waitExternal: 0,
  allowIdentifiableAi: false,
  // A browser test wallet (web: "Dev Wallet (localnet)"), localnet only.
  devWallet: false,
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
/** A switch: `--flag` alone (or true/1/on/yes) is on. */
const switchOn = v => v === true || ['true', '1', 'on', 'yes'].includes(String(v).toLowerCase());
const nonNegative = v => {
  const n = int(v);
  if (n < 0) throw new Error(`not a non-negative number: ${v}`);
  return n;
};
const optionalString = v => (v === undefined || v === null || v === '' ? null : String(v));

/** The default public port: the operator port plus this (4191 → 4194). */
export const PUBLIC_PORT_OFFSET = 3;

/** The gateway's options: config key, flag, environment variable, parser. Defaults come from DEFAULTS. */
export const OPTIONS = Object.freeze([
  ['cluster', '--cluster', 'PS_CLUSTER', String],
  ['baseRpc', '--base', 'PS_BASE', String],
  ['erRpc', '--er', 'PS_ER', String],
  ['programId', '--program', 'PS_PROGRAM', String],
  // The operator listener (always 127.0.0.1) and the public one (0 = off).
  ['port', '--port', 'PS_PORT', int],
  // Public port: default operator port + 3; `--no-trust-proxy` (or
  // PS_TRUST_PROXY=0) turns the X-Forwarded-For trust off.
  ['publicPort', '--public-port', 'PS_PUBLIC_PORT', v => (v === null ? null : nonNegative(v))],
  ['publicHost', '--public-host', 'PS_PUBLIC_HOST', String],
  ['trustProxy', '--trust-proxy', 'PS_TRUST_PROXY', switchOn],
  ['publicBaseRpc', '--public-base-rpc', 'PS_PUBLIC_BASE_RPC', optionalString],
  ['publicErRpc', '--public-er-rpc', 'PS_PUBLIC_ER_RPC', optionalString],
  ['minCrankSol', '--min-crank-sol', 'PS_MIN_CRANK_SOL', v => { const n = Number(v); if (!(n >= 0)) throw new Error(`not a SOL amount: ${v}`); return n; }],
  ['tickSeconds', '--tick-seconds', 'PS_TICK_SECONDS', int],
  ['commitEvery', '--commit-every', 'PS_COMMIT_EVERY', int],
  ['erValidator', '--er-validator', 'PS_ER_VALIDATOR', String],
  ['entryFee', '--entry-fee', 'PS_ENTRY_FEE', BigInt],
  // The USDC market (V5 §7.5) can be switched off per season.
  ['market', '--market', 'PS_MARKET', onOff],
  // Operator AI members per nation.
  ['ai', '--ai', 'PS_AI', nonNegative],
  ['bounty', '--bounty', 'PS_BOUNTY', BigInt],
  ['deposit', '--deposit', 'PS_DEPOSIT', BigInt],
  ['bond', '--bond', 'PS_BOND', v => (v === undefined || v === null || v === '' ? null : BigInt(v))],
  // Registration: open for `registrationSeconds`, then the season starts.
  // 0 = dev mode (see DEFAULTS), where it starts once `waitExternal`
  // members other than the AI members joined.
  ['registrationSeconds', '--registration-seconds', 'PS_REGISTRATION_SECONDS', nonNegative],
  ['waitExternal', '--wait-external', 'PS_WAIT_EXTERNAL', nonNegative],
  ['allowIdentifiableAi', '--allow-identifiable-ai', 'PS_ALLOW_IDENTIFIABLE_AI', switchOn],
  ['devWallet', '--dev-wallet', 'PS_DEV_WALLET', switchOn],
  // Several gateways (one per season) can share the stack and the keys.
  ['stateFile', '--state', 'PS_STATE', String],
  // The finalized season a new season follows in the history layer.
  ['prevSeason', '--prev-season', 'PS_PREV_SEASON', v => (v === undefined || v === null || v === '' ? null : String(v))],
].map(([key, flag, env, parse]) => Object.freeze({ key, flag, env, parse })));

/** A setting the gateway cannot run with (the message says what to do). */
export class ConfigError extends Error {}

/** Options that no longer exist, with what to do instead (passing one is an error). */
export const REMOVED_OPTIONS = Object.freeze([
  ['--humans', 'PS_HUMANS', 'gateway-hosted human seats were removed: people join with their own wallets (the web lobby, or x402); --ai sets the operator AI members'],
]);

const flagKey = flag => flag.slice(2).replace(/-([a-z])/g, (_, c) => c.toUpperCase());

/**
 * The gateway configuration: each option from its flag, else its
 * environment variable, else `defaults` (over DEFAULTS). The state file
 * resolves against .local/. `--new-season` sets `newSeason`. A removed
 * option (REMOVED_OPTIONS) throws with what to use instead.
 */
export function loadConfig({ argv = process.argv.slice(2), env = process.env, defaults = {} } = {}) {
  const args = parseArgs(argv);
  for (const [f, name, why] of REMOVED_OPTIONS) {
    if (args[flagKey(f)] !== undefined || env[name] !== undefined) throw new ConfigError(`${f} / ${name}: ${why}`);
  }
  const d = { ...DEFAULTS, ...defaults };
  const cfg = {};
  for (const { key, flag, env: name, parse } of OPTIONS) {
    const raw = args[flagKey(flag)] ?? env[name];
    cfg[key] = parse(raw === undefined ? d[key] : raw);
  }
  if (args.noTrustProxy === true) cfg.trustProxy = false;
  cfg.publicPort ??= cfg.port + PUBLIC_PORT_OFFSET;
  cfg.stateFile = path.resolve(LOCAL_DIR, cfg.stateFile);
  cfg.newSeason = args.newSeason === true;
  return cfg;
}

/**
 * Settings that cannot run (throws) and settings that do nothing (returned
 * as warnings). `creating`: a new season is about to be created with `cfg`.
 */
export function checkConfig(cfg, { creating = false } = {}) {
  const warnings = [];
  if (cfg.devWallet && cfg.cluster !== 'localnet') throw new ConfigError('--dev-wallet is for localnet only (a browser wallet whose key sits in the page)');
  if (cfg.publicPort && cfg.publicPort === cfg.port) throw new ConfigError(`--public-port ${cfg.publicPort} is the operator port`);
  if (cfg.port === 4190) throw new ConfigError('never port 4190 (browsers and fetch refuse it)');
  if (cfg.publicPort === 4190) {
    throw new ConfigError(cfg.publicPort === cfg.port + PUBLIC_PORT_OFFSET
      ? `--port ${cfg.port} puts the public listener on 4190 (the operator port + ${PUBLIC_PORT_OFFSET}), which browsers and fetch refuse: pass --public-port (another port, or 0 for none)`
      : 'never port 4190 (browsers and fetch refuse it)');
  }
  if (creating) {
    if (cfg.registrationSeconds === 0 && cfg.ai > 0 && !cfg.allowIdentifiableAi) {
      throw new ConfigError('--registration-seconds 0 registers the AI members first, so everyone can tell who they are; pass --allow-identifiable-ai to run it anyway (dev), or give people a registration window');
    }
    if (cfg.registrationSeconds > 0 && cfg.waitExternal > 0) warnings.push(`--wait-external ${cfg.waitExternal} is ignored: with --registration-seconds ${cfg.registrationSeconds} the season starts when registration closes, however many people joined`);
  }
  return warnings;
}

/** A named keypair persisted under .local/keys/<name>.json (created on first use). */
/**
 * The operator token: the game server presents it (on the loopback operator
 * listener only) to act for the operator's AI members and to read their
 * roster. From PS_OPERATOR_TOKEN, else .local/operator-token (created on
 * first use).
 */
export function operatorToken({ env = process.env, file = path.join(LOCAL_DIR, 'operator-token') } = {}) {
  if (env.PS_OPERATOR_TOKEN) return env.PS_OPERATOR_TOKEN;
  if (existsSync(file)) return readFileSync(file, 'utf8').trim();
  mkdirSync(path.dirname(file), { recursive: true });
  const t = randomBytes(24).toString('hex');
  writeFileSync(file, t + '\n', { mode: 0o600 });
  return t;
}

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
