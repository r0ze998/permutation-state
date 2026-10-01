// Wylls gateway (Game Design V5): HTTP front for the chain.
//
//   node src/server.mjs --new-season      create a season on the local stack, open its registration, serve
//        [--registration-seconds S]       registration window (default 600); the season starts when it
//                                         closes, however many people joined
//        [--ai 2]                         operator AI members per nation (they register at planned
//                                         times during the window, like anyone)
//        [--deposit D]                    the default treasury deposit everyone is offered
//        [--market off]                   a season without the USDC market (V5 §7.5)
//        [--port P --state file.json]     run several seasons side by side (the public listener
//                                         follows: P + 3 unless --public-port)
//        [--public-port Q --public-host H] the public listener (default: the operator port + 3,
//                                         4191 → 4194, on 127.0.0.1; 0 = none; see app.mjs)
//        [--no-trust-proxy]               the client address is the socket's peer even for
//                                         requests from this machine (default: a loopback peer's
//                                         X-Forwarded-For, i.e. the play server's /gw)
//        [--public-base-rpc URL --public-er-rpc URL]
//                                         the RPC URLs the public listener may publish (default: none)
//        [--min-crank-sol 0.3]            below it, co-signing, the faucet, /submit, /gov and the
//                                         AI registrations pause (503 OperatorLowFunds)
//        [--dev-wallet]                   localnet: let the web client offer its test wallet
//   node src/server.mjs                   resume the season in .local/season.json
//
// Dev mode: `--registration-seconds 0 --allow-identifiable-ai [--wait-external N]`
// registers the AI members at creation (so they are members 0…) and starts
// once N other members joined. `--humans` was removed: people join with
// their own wallets.
//
// Every option and its environment variable: config.mjs OPTIONS.
//
// Endpoints (JSON unless noted), one module each (the public listener
// serves app.mjs PUBLIC_ROUTES of them):
//   routes/season.mjs   GET /health, /season, /history, /world.bin, /ticks, /tick
//   routes/relay.mjs    POST /submit, /gov (operator); GET+POST /relay, /claim-relay
//   routes/x402.mjs     POST /x402/join
//   routes/seal.mjs     POST /seal
//   routes/faucet.mjs   POST /faucet, GET /usdc
//   routes/claims.mjs   GET /claims?wallet= (a wallet's prizes in this season and its lineage)
//   routes/roster.mjs   GET /roster; operator: GET /operator/roster, POST /roster/announce;
//                       GET+POST /talk (members' messages)
// Failures: routes/errors.mjs (a late submission is 409, a refused signer
// 403, a bad request 400, too many requests 429).
import http from 'node:http';
import { pathToFileURL } from 'node:url';
import { Connection } from '@solana/web3.js';
import { createApps } from './app.mjs';
import { checkConfig, ConfigError, createStateStore, DEFAULTS, loadConfig, operatorToken } from './config.mjs';
import { Crank } from './crank.mjs';
import { FundsGuard } from './guards.mjs';
import { Advisor, ADVISOR_MODEL } from './advisor.mjs';
import { bootstrap, createSeasonAccounts, registerPlannedNow } from './season.mjs';

const CRANK_INTERVAL_MS = 400;
/** A request must arrive within this on the public listener (slow senders are cut off). */
const PUBLIC_REQUEST_TIMEOUT_MS = 15_000;
const stamp = () => new Date().toISOString().slice(11, 19);

/** Listen on `host:port`; a port in use (or refused) is a ConfigError that says which flag to change. */
const listen = (server, port, host, what) => new Promise((resolve, reject) => {
  const fail = e => reject(e.code === 'EADDRINUSE' || e.code === 'EACCES'
    ? new ConfigError(`${what} ${host}:${port} is ${e.code === 'EADDRINUSE' ? 'in use (another gateway?)' : 'not allowed'}: pass ${what === 'public port' ? '--public-port (another port, or 0 for none)' : '--port'}`)
    : e);
  server.once('error', fail);
  server.listen(port, host, () => { server.off('error', fail); resolve(); });
});

/**
 * Check that the operator port and the public one (if any) are free, by
 * binding and closing them, before anything happens on chain: a gateway that
 * could not serve must not create a season. Throws ConfigError.
 */
export async function probePorts(cfg) {
  const ports = [[cfg.port, '127.0.0.1', 'operator port'], ...(cfg.publicPort ? [[cfg.publicPort, cfg.publicHost, 'public port']] : [])];
  for (const [port, host, what] of ports) {
    const probe = http.createServer();
    await listen(probe, port, host, what);
    await new Promise(resolve => probe.close(resolve));
  }
}

export async function main(argv = process.argv.slice(2)) {
  const cfg = loadConfig({ argv });
  // The game server presents this token to act for the operator's AI members (V5 §18.2).
  cfg.operatorToken ??= operatorToken();
  const base = new Connection(cfg.baseRpc, 'confirmed');
  const er = new Connection(cfg.erRpc, 'confirmed');
  const log = (...a) => console.log(stamp(), ...a);

  const store = createStateStore(cfg.stateFile);
  // Load first: a new season follows the finalized one this file held.
  const had = store.load();
  const creating = cfg.newSeason || !had;
  for (const w of checkConfig(cfg, { creating })) log(`warning: ${w}`);
  await probePorts(cfg);
  if (creating) {
    await bootstrap({ base, cfg, store, log });
  } else if (store.state.creating) {
    // A crash while creating: finish it (the plan was saved first).
    await createSeasonAccounts({ base, cfg, store, log });
    if (!store.state.registration?.seconds) for (const entry of store.state.aiPlan ?? []) await registerPlannedNow({ base, cfg, store, entry, log });
  }
  // One breaker for everything that makes the crank pay for a member: the
  // routes (people, agents, the game server's /submit and /gov for the AI
  // members) and the crank's own AI registrations pause together.
  const funds = FundsGuard.forSol({ read: () => base.getBalance(crank.crank.publicKey, 'confirmed'), minSol: cfg.minCrankSol ?? DEFAULTS.minCrankSol, log });
  const crank = new Crank({ base, er, cfg, store, log, funds });
  const timer = setInterval(() => crank.step(), CRANK_INTERVAL_MS);
  const advisor = new Advisor();
  if (advisor.available) log(`AI members' answers phrased by ${ADVISOR_MODEL} (operator's key, at most ${advisor.budget} per run)`);
  const apps = createApps({ cfg, base, er, store, crank, log, advisor, funds });
  // The operator listener never leaves this machine: the game server uses
  // it with the operator token.
  const server = http.createServer(apps.operator);
  let publicServer = null;
  try {
    await listen(server, cfg.port, '127.0.0.1', 'operator port');
    if (cfg.publicPort) {
      publicServer = http.createServer(apps.public);
      publicServer.requestTimeout = PUBLIC_REQUEST_TIMEOUT_MS;
      publicServer.headersTimeout = PUBLIC_REQUEST_TIMEOUT_MS;
      await listen(publicServer, cfg.publicPort, cfg.publicHost, 'public port');
    }
  } catch (e) {
    clearInterval(timer);
    server.close();
    throw e;
  }
  server.publicServer = publicServer;
  server.crank = crank;
  server.on('close', () => { clearInterval(timer); publicServer?.close(); });
  const reg = store.state.registration;
  log(`gateway on http://127.0.0.1:${cfg.port} (operator)${publicServer ? ` and http://${cfg.publicHost}:${cfg.publicPort} (public${cfg.trustProxy
    ? ', a local proxy\'s X-Forwarded-For names the client' : ', every client limited by its socket address (--no-trust-proxy)'}${cfg.publicBaseRpc || cfg.publicErRpc ? '' : ', RPC URLs not published'})` : ''}`
    + ` season ${store.state.seasonId} (${crank.phase}${reg?.closesAt && crank.phase === 'registering' ? `, registration closes ${new Date(reg.closesAt).toISOString()}` : ''})`);
  return server;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch(e => { console.error(e instanceof ConfigError ? `error: ${e.message}` : e); process.exit(1); });
}
