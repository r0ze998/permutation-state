# @permutation/game-client

Client for PERMUTATION STATE agents and tools (Game Design V5), over plain HTTP. An agent is a **member of a nation** with exactly the rights a person has: it can stand for office, vote, propose, support, recall and, in office, order.

- **Read** your nation's fogged view, previews and dry-run validation from the game server.
- **Join** a nation by paying the entry fee over HTTP 402 (x402, scheme `exact`). The payment is the program's own `Register` instruction, signed by your wallet.
- **Govern**: stand, vote, propose orders to an office, support proposals, recall an officer. Each action is signed with your session key and relayed through the gateway, which only pays the fee.
- **Order** as an officer: one sealed batch per office you hold, checked by the program on the MagicBlock Ephemeral Rollup.
- **Commit and reveal** your reasoning. Each batch carries a digest of your observation root, policy name and salted rationale; the client reveals it in a later batch of the same office.
- **Claim** your prize and treasury share after the season, signed by your wallet (the gateway pays the fee).
- **MCP server** (`permutation-mcp`), so any MCP client can play.

Not published to npm yet. Inside this repository it resolves `@solana/web3.js` and the MagicBlock SDK from `permutation-gateway/node_modules` (`npm ci` there once).

Defaults: game server `http://127.0.0.1:4185`, gateway `http://127.0.0.1:4191`. (Not 4190: browsers and Node's `fetch` refuse that port.)

## Use

```js
import { GameClient, loadOrCreateKeypair } from './client/src/index.mjs';

const game = new GameClient({ server: 'http://127.0.0.1:4185', gateway: 'http://127.0.0.1:4191' });
const wallet = await loadOrCreateKeypair('.local/me/wallet.json');   // pays the entry fee, receives the prize
const session = await loadOrCreateKeypair('.local/me/session.json'); // signs orders and governance, cannot move USDC
game.session = session;

const { usdcAccount } = await game.faucet(wallet.publicKey);         // localnet test USDC (no value)
const joined = await game.joinViaX402({ wallet, session, civ: 4, name: 'Hypatia', usdcAccount,
  stand: ['Science', 'Diplomat'] });                                 // candidacy for the first election
console.log(joined.member, joined.nation, joined.paymentResponse);   // X-PAYMENT-RESPONSE, decoded

for (;;) {
  const v = await game.state();                                      // your nation's fogged view + government
  if (v.over || v.chain?.finished) break;
  // Governance first: once every office's batch is in, the tick's input freezes.
  if (v.gov.voteOpen) await game.vote('Science', game.member);
  const r = await game.submit({ orders: [{ type: 'SetResearch', techs: ['Writing'] }], policy: 'my-agent/v1', rationale: 'Science first.', view: v });
  for (const o of r.notHeld) console.log('not my office:', o.type);  // propose these next tick with game.propose(role, [o])
  await game.waitForTick(v.tick);
}
await game.claim({ wallet, usdcAccount });                           // once the season is Finalized on base
```

`submit` does everything the chain needs, for each office you hold:

1. It commits to your decision against `view.decision.obsRoot`.
2. It adds `RevealRationale` orders for that office's earlier decisions (at most 3).
3. It validates the batch.
4. It signs `SubmitOrders` with the session key. An office with nothing to do still sends an empty sealed batch: that ends its turn, so the tick can resolve as soon as every office is in, and it keeps the office from counting as abandoned.
5. It posts the batch to the gateway's `/relay`.

Orders for offices you do not hold come back in `notHeld`; send them as proposals (`propose(role, orders)`).

**Timing.** A tick resolves at its deadline, or as soon as every office of every nation has submitted. At that moment its input is frozen and published on chain; later submissions for that tick get HTTP 409 (`TickFrozen` or `WrongTick`, see `chainError`). Send governance actions before your own batch, and retry anything refused on the next tick.

## Methods

| Method | What |
|---|---|
| `lobby()`, `map()`, `state()` | nations and members; the static map; your nation's fogged view with `gov` (offices, candidates, proposals, recalls) |
| `preview(kind, params)`, `validate(orders)` | the same previews and dry-run a person sees |
| `season()` | the gateway's season: members, accounts, genesis and seating records |
| `faucet(owner)` | localnet only: a token account with 100 test USDC |
| `joinViaX402({ wallet, session, civ, name, stand, votes, deposit })` | register over HTTP 402 |
| `submit({ orders, policy, rationale, adopt, view })` | sealed batches for the offices you hold |
| `propose`, `support`, `vote`, `stand`, `recall`, `gov(action)` | governance actions |
| `myOffices(view)`, `waitForTick(tick)` | helpers |
| `claim({ wallet, usdcAccount })` | prize and treasury share after the season |

## MCP

```json
{
  "mcpServers": {
    "permutation-state": {
      "command": "node",
      "args": ["<repo>/permutation-gateway/client/bin/permutation-mcp.mjs"],
      "env": { "PS_SERVER": "http://127.0.0.1:4185", "PS_GATEWAY": "http://127.0.0.1:4191", "PS_AGENT_DIR": "/path/to/keys" }
    }
  }
}
```

Tools:

- `get_rules`, `get_state`
- `preview_unit`, `preview_city`, `preview_research`, `preview_diplomacy`
- `find_path`, `validate_orders`, `submit_orders`
- `propose`, `govern`
- `join_season` (x402), `wait_for_next_tick`

## Reference agents

- [`../agents/rule-agent.mjs`](../agents/rule-agent.mjs) is rule-based and needs no model. Its priorities are research, then founding and expanding, scouting the fog, favourable fights only, and keeping every city building. Orders outside its offices become proposals; it supports nation-mates' proposals that would still work and votes in every election.
- [`../agents/llm-agent.mjs`](../agents/llm-agent.mjs) is Claude with the same tools as MCP, and needs `ANTHROPIC_API_KEY`. Its batch is validated before it is accepted, and errors go back to the model.

Both use [`../agents/runner.mjs`](../agents/runner.mjs). The runner joins through x402, keeps the keys, the membership and any unrevealed commitments in `--dir`, plays until the season ends, and then claims the prize.

## Files

| File | What |
|---|---|
| `src/game.mjs` | `GameClient`: HTTP, x402 entry, signing and relay, governance, claim |
| `src/decision.mjs` | commit-reveal digests, byte-identical to `permutation-rules::decision` |
| `src/summary.mjs` | compact state for prompts |
| `src/tools.mjs` | tool definitions shared by MCP and the LLM agent |
| `src/mcp.mjs`, `bin/permutation-mcp.mjs` | MCP server (stdio) |
| `src/codec.mjs`, `src/borsh.mjs` | borsh encoding of orders, governance actions and instructions; decoding of accounts and log records; program error names (tested against Rust vectors) |
| `src/chain.mjs`, `src/pda.mjs` | instruction builders and PDAs of the `permutation-chain` program |
