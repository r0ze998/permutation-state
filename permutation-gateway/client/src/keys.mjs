// Keypairs persisted as Solana CLI style JSON arrays (the secret key bytes),
// created on first use with owner-only permissions.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { Keypair } from '@solana/web3.js';

export function loadOrCreateKeypairSync(file) {
  if (existsSync(file)) return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(file, 'utf8'))));
  mkdirSync(path.dirname(file), { recursive: true });
  const kp = Keypair.generate();
  writeFileSync(file, JSON.stringify(Array.from(kp.secretKey)), { mode: 0o600 });
  return kp;
}

/** A keypair persisted at `file` (created on first use). */
export async function loadOrCreateKeypair(file) {
  return loadOrCreateKeypairSync(file);
}
