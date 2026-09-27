// PERMUTATION STATE: Sixfold Frontier (M1) relay — free, sponsored player
// transactions for one season of the `permutation-frontier` program
// (contract §8.3). No money: the relay pool pays fees, the Citizen's rent at
// Join, the Holding-rent escrow at FileTicket and the Depart escrow, all
// test SOL, within per-citizen quotas.
//
//   node src/frontier/server.mjs --program <id> --season <n>
//        [--rpc http://127.0.0.1:41010]         the chain (frontier-localnet in the local stack)
//        [--port 41030 --public-port 41033]     operator and public listeners (both 127.0.0.1;
//                                               M1 ports only: 41000–41999, never a reserved one)
//        [--herald-peer 127.0.0.1]              the herald's loopback address (its X-Forwarded-For is trusted)
//        [--herald http://127.0.0.1:41040]      published in GET /f/season
//        [--keeper http://127.0.0.1:41050 --keeper-token-file F]   the keeper link (/f/reveal, /f/nudge)
//        [--pool-size 150 --master-seed-file F] the relay pool (pool_id "relay"; < 150 only with --dev)
//        [--gate-key-file F]                    the join-gate keypair (playtest preset; invites)
//        [--invite-secret-file F --state-file F --min-pool-sol 1 --dev]
//   Every flag also as FRONTIER_* (config.mjs); the operator token from
//   FRONTIER_OPERATOR_TOKEN or .local/frontier/operator-token.
//
// The relay starts no chain and sends no transaction of its own; it only
// co-signs what players built, after the checks in routes/relay.mjs.
import http from 'node:http';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { Connection } from '@solana/web3.js';
import { loadOrCreateKeypairSync } from '../../client/src/keys.mjs';
import { createStateStore, operatorToken } from '../config.mjs';
import { createFrontierApps } from './app.mjs';
import { FRONTIER_DIR, loadFrontierConfig, secretFile } from './config.mjs';
import { InviteBook } from './invites.mjs';
import { KeeperLink } from './keeperlink.mjs';
import { PayerPool, RELAY_POOL_ID } from './payers.mjs';

const listen = (server, port, host) => new Promise((resolve, reject) => {
  server.once('error', reject);
  server.listen(port, host, () => { server.off('error', reject); resolve(server.address().port); });
});

/** Build and start the relay; returns `{ctx, servers, ports, close()}`. */
export async function startFrontierRelay(cfg, { connection = new Connection(cfg.rpc, 'confirmed'), log = console.log } = {}) {
  const store = createStateStore(cfg.stateFile);
  store.load();
  store.state ??= {};
  const pool = new PayerPool({ masterSeed: secretFile(cfg.masterSeedFile), n: cfg.poolSize, poolId: RELAY_POOL_ID, dev: cfg.dev, minLamports: cfg.minPayerLamports });
  const gateKey = cfg.gateKeyFile ? loadOrCreateKeypairSync(cfg.gateKeyFile) : null;
  const invites = new InviteBook({ secret: secretFile(cfg.inviteSecretFile), seasonId: cfg.seasonId, store });
  const keeper = cfg.keeperUrl ? new KeeperLink({ url: cfg.keeperUrl, tokenFile: cfg.keeperTokenFile }) : null;
  const apps = createFrontierApps({ cfg, connection, pool, keeper, invites, gateKey, store, log });
  const operator = http.createServer(apps.operator);
  const pub = cfg.publicPort ? http.createServer(apps.public) : null;
  const ports = { operator: await listen(operator, cfg.port, '127.0.0.1') };
  if (pub) {
    pub.requestTimeout = 15_000;
    ports.public = await listen(pub, cfg.publicPort, cfg.publicHost);
  }
  log(`frontier relay: program ${cfg.programId} season ${cfg.seasonId} (${apps.ctx.addresses.season}); pool ${pool.size} payers; `
    + `operator 127.0.0.1:${ports.operator}${pub ? `, public ${cfg.publicHost}:${ports.public}` : ''}${gateKey ? `; join gate ${gateKey.publicKey.toBase58()}` : ''}`);
  const close = () => Promise.all([operator, pub].filter(Boolean).map(s => new Promise(r => s.close(r))));
  return { ctx: apps.ctx, servers: { operator, public: pub }, ports, close };
}

async function main() {
  const cfg = loadFrontierConfig(process.argv.slice(2), process.env);
  cfg.operatorToken ??= operatorToken({ file: path.join(FRONTIER_DIR, 'operator-token') });
  const relay = await startFrontierRelay(cfg);
  const stop = () => relay.close().then(() => process.exit(0));
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch(e => {
    console.error(e.message);
    process.exit(1);
  });
}
