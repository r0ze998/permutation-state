// Gateway configuration and local key storage. Keys live under .local/keys
// (git-ignored). They are disposable localnet/devnet keys; the gateway never
// creates or funds mainnet keys.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Keypair } from '@solana/web3.js';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const LOCAL_DIR = path.join(ROOT, '.local');
const KEYS = path.join(LOCAL_DIR, 'keys');

export function loadConfig(argv = process.argv) {
  const arg = (k, d) => { const i = argv.indexOf(k); return i > 0 ? argv[i + 1] : (process.env[`PS_${k.slice(2).toUpperCase().replace(/-/g, '_')}`] ?? d); };
  return {
    cluster: arg('--cluster', 'localnet'),
    baseRpc: arg('--base', 'http://127.0.0.1:18899'),
    erRpc: arg('--er', 'http://127.0.0.1:17799'),
    programId: arg('--program', 'J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n'),
    // Not 4190: browsers and Node's fetch refuse it (a "bad port" in the Fetch standard).
    port: Number(arg('--port', '4191')),
    tickSeconds: Number(arg('--tick-seconds', '30')),
    // Periodic ER→base commits (0 = none; the final undelegation always
    // commits). 180 ticks / 20 = 9, within the ER's 10 sponsored commits per
    // account. Each round goes out in small `CommitPart` intents.
    commitEvery: Number(arg('--commit-every', '20')),
    erValidator: arg('--er-validator', 'mAGicPQYBMvcYveUZA5F5UNNwyHvfYh5xkLS2Fr1mev'),
    entryFee: BigInt(arg('--entry-fee', '10000000')), // 10 USDC (6 decimals)
    // The USDC market (V5 §7.5) can be switched off per season.
    market: arg('--market', 'on') !== 'off',
    // Hosted members: claimable human members, and AI members per nation.
    humans: Number(arg('--humans', '1')),
    ai: Number(arg('--ai', '2')),
    // Registration closes once this many outside members joined (x402), or
    // after `registrationSeconds`, whichever comes first.
    waitExternal: Number(arg('--wait-external', '0')),
    registrationSeconds: Number(arg('--registration-seconds', '0')),
    // Several gateways (one per season) can share the stack and the keys.
    stateFile: path.resolve(LOCAL_DIR, arg('--state', 'season.json')),
  };
}

/** A named keypair persisted under .local/keys/<name>.json (created on first use). */
export function namedKey(name) {
  mkdirSync(KEYS, { recursive: true });
  const file = path.join(KEYS, `${name}.json`);
  if (existsSync(file)) return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(file, 'utf8'))));
  const kp = Keypair.generate();
  writeFileSync(file, JSON.stringify(Array.from(kp.secretKey)), { mode: 0o600 });
  return kp;
}

let stateFile = path.join(LOCAL_DIR, 'season.json');

export function readState(file = stateFile) {
  stateFile = file;
  return existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')) : null;
}

export function writeState(state) {
  mkdirSync(path.dirname(stateFile), { recursive: true });
  writeFileSync(stateFile, JSON.stringify(state, null, 2));
}
