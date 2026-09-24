// A Model Context Protocol server (stdio, JSON-RPC 2.0, one message per line)
// exposing PERMUTATION STATE to any MCP client. Tools: see tools.mjs, plus
// join_season (x402 entry) and wait_for_next_tick.
import path from 'node:path';
import os from 'node:os';
import { createInterface } from 'node:readline';
import { GameClient, loadOrCreateKeypair } from './game.mjs';
import { TOOLS, callTool, RULES_BRIEF } from './tools.mjs';

const EXTRA = [
  { name: 'join_season', description: 'Join the open season as a new civilization: pays the entry fee in (test) USDC over HTTP 402 and registers your session key. Needed once, unless PS_CIV is configured.', inputSchema: { type: 'object', properties: { name: { type: 'string', description: 'your civilization name (letters only, ≤16)' } }, required: ['name'] } },
  { name: 'wait_for_next_tick', description: 'Block until the current tick resolves, then return the new state summary.', inputSchema: { type: 'object', properties: {} } },
];

export async function runMcp({ env = process.env, input = process.stdin, output = process.stdout } = {}) {
  const dir = env.PS_AGENT_DIR || path.join(os.homedir(), '.permutation-agent');
  const game = new GameClient({ server: env.PS_SERVER, gateway: env.PS_GATEWAY, civ: env.PS_CIV !== undefined ? Number(env.PS_CIV) : null });
  if (game.civ !== null) game.session = await loadOrCreateKeypair(path.join(dir, 'session.json'));
  const ctx = { game, policy: env.PS_POLICY || 'mcp-agent/v1' };
  const send = m => output.write(`${JSON.stringify(m)}\n`);

  async function call(name, args) {
    if (name === 'join_season') {
      const wallet = await loadOrCreateKeypair(path.join(dir, 'wallet.json'));
      const session = await loadOrCreateKeypair(path.join(dir, 'session.json'));
      const f = await game.faucet(wallet.publicKey);
      return game.joinViaX402({ wallet, session, name: args.name, kind: 1, usdcAccount: f.usdcAccount });
    }
    if (name === 'wait_for_next_tick') {
      const now = await game.state();
      await game.waitForTick(now.tick);
      return callTool(ctx, 'get_state', {});
    }
    if (game.civ === null && name !== 'get_rules') return { error: 'no civilization yet: call join_season first' };
    return callTool(ctx, name, args);
  }

  const rl = createInterface({ input, crlfDelay: Infinity });
  for await (const line of rl) {
    if (!line.trim()) continue;
    let msg;
    try { msg = JSON.parse(line); } catch { send({ jsonrpc: '2.0', id: null, error: { code: -32700, message: 'parse error' } }); continue; }
    const { id, method, params } = msg;
    if (id === undefined) continue; // notifications (initialized, cancelled) need no answer
    try {
      let result;
      switch (method) {
        case 'initialize':
          result = { protocolVersion: params?.protocolVersion || '2025-06-18', capabilities: { tools: { listChanged: false } },
            serverInfo: { name: 'permutation-state', version: '0.1.0' }, instructions: RULES_BRIEF };
          break;
        case 'ping': result = {}; break;
        case 'tools/list': result = { tools: [...TOOLS, ...EXTRA] }; break;
        case 'tools/call': {
          const out = await call(params?.name, params?.arguments || {}).catch(e => ({ error: e.message }));
          result = { content: [{ type: 'text', text: JSON.stringify(out) }], isError: Boolean(out?.error) };
          break;
        }
        default:
          send({ jsonrpc: '2.0', id, error: { code: -32601, message: `method not found: ${method}` } });
          continue;
      }
      send({ jsonrpc: '2.0', id, result });
    } catch (e) {
      send({ jsonrpc: '2.0', id, error: { code: -32603, message: e.message } });
    }
  }
}
