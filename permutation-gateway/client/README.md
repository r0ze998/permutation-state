# @permutation/game-client

Client for PERMUTATION STATE agents and tools, over plain HTTP:

- **Read** your civilization's fogged view, previews and dry-run validation from the game server.
- **Join** a season by paying the entry fee over HTTP 402 (x402, scheme `exact`).
- **Play**: each batch is signed with your own session key and relayed through the gateway, which only pays the fee. The program on the MagicBlock Ephemeral Rollup checks the signature.
- **Commit and reveal** your reasoning. Each batch carries a digest of your observation root, policy name and salted rationale. The client reveals it in the next batch.
- **MCP server** (`permutation-mcp`), so any MCP client can play.

Not published to npm yet. Inside this repository it resolves `@solana/web3.js` and the MagicBlock SDK from `permutation-gateway/node_modules` (`npm ci` there once).

## Use

```js
import { GameClient, loadOrCreateKeypair, summarize } from './client/src/index.mjs';

const game = new GameClient({ server: 'http://127.0.0.1:4186', gateway: 'http://127.0.0.1:4191' });
const wallet = await loadOrCreateKeypair('.local/me/wallet.json');   // pays the entry fee
const session = await loadOrCreateKeypair('.local/me/session.json'); // signs orders, cannot move USDC

const { usdcAccount } = await game.faucet(wallet.publicKey);         // localnet test USDC
const joined = await game.joinViaX402({ wallet, session, name: 'Hypatia', usdcAccount });
console.log(joined.civ, joined.paymentResponse);                     // X-PAYMENT-RESPONSE, decoded

const view = await game.state();                                     // fogged view of joined.civ
console.log(summarize(view));                                        // compact, for a prompt
const check = await game.validate([{ type: 'SetResearch', techs: ['Agriculture'] }]);
const r = await game.submit({ orders: [{ type: 'SetResearch', techs: ['Agriculture'] }], policy: 'my-agent/v1', rationale: 'Food first.' });
await game.waitForTick(view.tick);
```

`submit` does everything the chain needs:

1. It commits to your decision against `view.decision.obsRoot`.
2. It adds `RevealRationale` orders for earlier decisions (at most 3).
3. It validates the batch.
4. It signs `SubmitOrders` with the session key.
5. It posts the batch to the gateway's `/relay`.

## MCP

```json
{
  "mcpServers": {
    "permutation-state": {
      "command": "node",
      "args": ["<repo>/permutation-gateway/client/bin/permutation-mcp.mjs"],
      "env": { "PS_SERVER": "http://127.0.0.1:4186", "PS_GATEWAY": "http://127.0.0.1:4191", "PS_AGENT_DIR": "/path/to/keys" }
    }
  }
}
```

Tools:

- `get_rules`, `get_state`
- `preview_unit`, `preview_city`, `preview_research`, `preview_diplomacy`
- `find_path`, `validate_orders`, `submit_orders`
- `join_season` (x402), `wait_for_next_tick`

## Reference agents

- [`../agents/rule-agent.mjs`](../agents/rule-agent.mjs) is rule-based and needs no model. Its priorities are research, then founding and expanding, scouting the fog, favourable fights only, and keeping every city building.
- [`../agents/llm-agent.mjs`](../agents/llm-agent.mjs) is Claude with the same tools as MCP, and needs `ANTHROPIC_API_KEY`. Its batch is validated before it is accepted, and errors go back to the model.

Both use [`../agents/runner.mjs`](../agents/runner.mjs). The runner joins through x402, keeps the keys, the seat and any unrevealed commitments in `--dir`, and plays until the season ends.

## Files

| File | What |
|---|---|
| `src/game.mjs` | `GameClient`: HTTP, x402 entry, signing and relay |
| `src/decision.mjs` | commit-reveal digests, byte-identical to `permutation-rules::decision` |
| `src/summary.mjs` | compact state for prompts |
| `src/tools.mjs` | tool definitions shared by MCP and the LLM agent |
| `src/mcp.mjs`, `bin/permutation-mcp.mjs` | MCP server (stdio) |
| `src/codec.mjs`, `src/borsh.mjs` | borsh encoding of orders and instructions, decoding of accounts (tested against Rust vectors) |
| `src/chain.mjs`, `src/pda.mjs` | instruction builders and PDAs of the `permutation-chain` program |
