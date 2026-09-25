// PERMUTATION STATE gateway (Game Design V5): HTTP front for the chain.
//
//   node src/server.mjs --new-season      bootstrap a season on the local stack, then serve
//        [--humans 1 --ai 2]              hosted members: claimable humans, AI members per nation
//        [--wait-external N]              keep registration open until N outside members joined (x402)
//        [--registration-seconds S]       …or at most S seconds
//        [--market off]                   a season without the USDC market (V5 §7.5)
//        [--port P --state file.json]     run several seasons side by side
//   node src/server.mjs                   resume the season in .local/season.json
//
// Every option and its environment variable: config.mjs OPTIONS.
//
// Endpoints (JSON unless noted), one module each:
//   routes/season.mjs   GET /health, /season, /history, /world.bin, /ticks, /tick
//   routes/relay.mjs    POST /submit, /gov (operator); GET+POST /relay, /claim-relay
//   routes/x402.mjs     POST /x402/join
//   routes/faucet.mjs   POST /faucet
//   routes/roster.mjs   GET /roster; operator: GET /operator/roster, POST /roster/announce;
//                       GET+POST /talk (members' messages)
// Failures: routes/errors.mjs (a late submission is 409, a refused signer
// 403, a bad request 400).
import http from 'node:http';
import { pathToFileURL } from 'node:url';
import { Connection } from '@solana/web3.js';
import { createApp } from './app.mjs';
import { createStateStore, loadConfig, operatorToken } from './config.mjs';
import { Crank } from './crank.mjs';
import { Advisor, ADVISOR_MODEL } from './advisor.mjs';
import { bootstrap, defaultRoster } from './season.mjs';

const CRANK_INTERVAL_MS = 400;
const stamp = () => new Date().toISOString().slice(11, 19);

export async function main(argv = process.argv.slice(2)) {
  const cfg = loadConfig({ argv });
  // The game server presents this token to act for hosted members (V5 §18.2).
  cfg.operatorToken ??= operatorToken();
  const base = new Connection(cfg.baseRpc, 'confirmed');
  const er = new Connection(cfg.erRpc, 'confirmed');
  const log = (...a) => console.log(stamp(), ...a);

  const store = createStateStore(cfg.stateFile);
  // Load first: a new season follows the finalized one this file held.
  const had = store.load();
  if (cfg.newSeason || !had) {
    const roster = defaultRoster({ humans: cfg.humans, ai: cfg.ai }).map(m => ({ ...m, deposit: cfg.deposit }));
    await bootstrap({ base, cfg, store, roster, log });
  }
  const crank = new Crank({ base, er, cfg, store, log });
  const timer = setInterval(() => crank.step(), CRANK_INTERVAL_MS);
  const advisor = new Advisor();
  if (advisor.available) log(`AI members' answers phrased by ${ADVISOR_MODEL} (operator's key, at most ${advisor.budget} per run)`);
  const server = http.createServer(createApp({ cfg, base, er, store, crank, log, advisor }));
  server.on('close', () => clearInterval(timer));
  await new Promise(resolve => server.listen(cfg.port, '127.0.0.1', resolve));
  log(`gateway on http://127.0.0.1:${cfg.port} season ${store.state.seasonId} (${crank.phase})`);
  return server;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch(e => { console.error(e); process.exit(1); });
}
