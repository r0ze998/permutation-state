// The Frontier relay's configuration (contract §8.3, §10.3): one table of
// defaults, the flag/environment parser, and the port rule every M1 service
// follows — ports in 41000–41999, never one of the owner's reserved ports,
// and the public listener on loopback only (the herald proxies it as
// `/gw/*`, I-51).
import { randomBytes } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { LOCAL_DIR } from '../config.mjs';

export const FRONTIER_DIR = path.join(LOCAL_DIR, 'frontier');

/** Ports the owner's services use; no M1 process binds one (§10.3, I-26). */
export const RESERVED_PORTS = Object.freeze([4185, 4190, 4191, 4194, 18899, 17799, 28899, 27799, 26699, 5185, 5191]);
export const M1_PORT_MIN = 41000;
export const M1_PORT_MAX = 41999;

export const FRONTIER_DEFAULTS = Object.freeze({
  cluster: 'localnet',
  rpc: 'http://127.0.0.1:41010',
  programId: null,
  seasonId: null,
  /** Operator listener (loopback, every route) and public listener (loopback, the herald's /gw). */
  port: 41030,
  publicPort: 41033,
  publicHost: '127.0.0.1',
  /** The herald's loopback address: only its requests may carry a trusted X-Forwarded-For. */
  heraldPeer: '127.0.0.1',
  heraldUrl: 'http://127.0.0.1:41040',
  keeperUrl: 'http://127.0.0.1:41050',
  keeperTokenFile: null,
  /** Relay payer pool: 150 keys derived from the master seed, `pool_id = "relay"` (I-49 scheme). */
  poolSize: 150,
  masterSeedFile: path.join(FRONTIER_DIR, 'relay-master.seed'),
  /** The join-gate key (playtest preset: its public key is `Season.join_gate`). */
  gateKeyFile: null,
  inviteSecretFile: path.join(FRONTIER_DIR, 'invite.secret'),
  stateFile: path.join(FRONTIER_DIR, 'relay-state.json'),
  /** Public co-signing pauses (503 OperatorLowFunds) below this pool total. */
  minPoolSol: 1,
  /** A payer below this is not drawn for GET /f/relay. */
  minPayerLamports: 50_000_000,
  /** Allow a pool below 150 keys (tests, dev stacks). */
  dev: false,
});

/** Whether an M1 service may bind `port` (§10.3); the reason if not. */
export function portProblem(port, what = 'port') {
  if (!Number.isInteger(port)) return `${what} ${port} is not a port`;
  if (port === 0) return null; // tests: an ephemeral port on 127.0.0.1
  if (RESERVED_PORTS.includes(port)) return `${what} ${port} is reserved (owner's services)`;
  if (port < M1_PORT_MIN || port > M1_PORT_MAX) return `${what} ${port} is outside ${M1_PORT_MIN}–${M1_PORT_MAX}`;
  return null;
}

const LOOPBACK = new Set(['127.0.0.1', '::1', 'localhost']);

/**
 * The relay's configuration from flags (`--rpc URL --program ID --season N
 * --port P --public-port Q --herald-peer A --keeper URL --keeper-token-file F
 * --pool-size N --master-seed-file F --gate-key-file F --invite-secret-file F
 * --state-file F --min-pool-sol X --dev`) over FRONTIER_* environment
 * variables over the defaults. Throws on a bad value.
 */
export function loadFrontierConfig(argv = [], env = {}) {
  const flags = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith('--')) throw new Error(`unexpected argument ${a}`);
    const name = a.slice(2);
    if (name === 'dev') { flags.dev = true; continue; }
    if (i + 1 >= argv.length) throw new Error(`${a} needs a value`);
    flags[name] = argv[++i];
  }
  const pick = (flag, envName, dflt) => flags[flag] ?? env[envName] ?? dflt;
  const int = (v, what) => {
    const n = Number(v);
    if (!Number.isInteger(n) || n < 0) throw new Error(`${what}: ${v} is not a non-negative integer`);
    return n;
  };
  const d = FRONTIER_DEFAULTS;
  const cfg = {
    cluster: pick('cluster', 'FRONTIER_CLUSTER', d.cluster),
    rpc: pick('rpc', 'FRONTIER_RPC', d.rpc),
    programId: pick('program', 'FRONTIER_PROGRAM_ID', d.programId),
    seasonId: pick('season', 'FRONTIER_SEASON_ID', d.seasonId),
    port: int(pick('port', 'FRONTIER_RELAY_PORT', d.port), '--port'),
    publicPort: int(pick('public-port', 'FRONTIER_RELAY_PUBLIC_PORT', d.publicPort), '--public-port'),
    publicHost: pick('public-host', 'FRONTIER_RELAY_PUBLIC_HOST', d.publicHost),
    heraldPeer: pick('herald-peer', 'FRONTIER_HERALD_PEER', d.heraldPeer),
    heraldUrl: pick('herald', 'FRONTIER_HERALD_URL', d.heraldUrl),
    keeperUrl: pick('keeper', 'FRONTIER_KEEPER_URL', d.keeperUrl),
    keeperTokenFile: pick('keeper-token-file', 'FRONTIER_KEEPER_TOKEN_FILE', d.keeperTokenFile),
    poolSize: int(pick('pool-size', 'FRONTIER_RELAY_POOL', d.poolSize), '--pool-size'),
    masterSeedFile: pick('master-seed-file', 'FRONTIER_RELAY_SEED_FILE', d.masterSeedFile),
    gateKeyFile: pick('gate-key-file', 'FRONTIER_GATE_KEY_FILE', d.gateKeyFile),
    inviteSecretFile: pick('invite-secret-file', 'FRONTIER_INVITE_SECRET_FILE', d.inviteSecretFile),
    stateFile: pick('state-file', 'FRONTIER_RELAY_STATE', d.stateFile),
    minPoolSol: Number(pick('min-pool-sol', 'FRONTIER_MIN_POOL_SOL', d.minPoolSol)),
    minPayerLamports: int(pick('min-payer-lamports', 'FRONTIER_MIN_PAYER_LAMPORTS', d.minPayerLamports), '--min-payer-lamports'),
    dev: !!(flags.dev ?? (env.FRONTIER_DEV === '1' || d.dev)),
    operatorToken: env.FRONTIER_OPERATOR_TOKEN ?? null,
  };
  const bad = checkFrontierConfig(cfg);
  if (bad.length) throw new Error(`frontier relay config: ${bad.join('; ')}`);
  return cfg;
}

/** Problems with a configuration (empty when it may run). */
export function checkFrontierConfig(cfg) {
  const out = [];
  for (const [p, what] of [[cfg.port, '--port'], [cfg.publicPort, '--public-port']]) {
    const e = portProblem(p, what);
    if (e) out.push(e);
  }
  if (cfg.port && cfg.port === cfg.publicPort) out.push('--port and --public-port must differ');
  // I-51: the public listener is reached through the herald's /gw proxy only.
  if (!LOOPBACK.has(cfg.publicHost)) out.push(`--public-host ${cfg.publicHost}: the public listener binds loopback only (the herald proxies it)`);
  if (!LOOPBACK.has(cfg.heraldPeer) && !/^127\./.test(cfg.heraldPeer)) out.push(`--herald-peer ${cfg.heraldPeer} must be a loopback address`);
  if (!cfg.dev && cfg.poolSize < 150) out.push(`--pool-size ${cfg.poolSize}: the relay pool has at least 150 keys (--dev for less)`);
  if (cfg.poolSize < 1) out.push('--pool-size must be at least 1');
  if (!cfg.programId) out.push('--program is required');
  if (cfg.seasonId === null || cfg.seasonId === undefined || !/^\d{1,20}$/.test(String(cfg.seasonId))) out.push('--season (the season id) is required');
  if (!(cfg.minPoolSol >= 0)) out.push('--min-pool-sol must be ≥ 0');
  return out;
}

/** A 32-byte secret kept in `file` (created with mode 0600 the first time). */
export function secretFile(file, { create = true } = {}) {
  if (existsSync(file)) {
    const t = readFileSync(file, 'utf8').trim();
    if (!/^[0-9a-f]{64}$/.test(t)) throw new Error(`${file}: not 32 bytes of hex`);
    return Buffer.from(t, 'hex');
  }
  if (!create) throw new Error(`${file} does not exist`);
  mkdirSync(path.dirname(file), { recursive: true });
  const b = randomBytes(32);
  writeFileSync(file, `${b.toString('hex')}\n`, { mode: 0o600 });
  return b;
}
