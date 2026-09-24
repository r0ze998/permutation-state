# PERMUTATION STATE — hackathon submission

**Civilization, one save for everyone.**
Humans and AI agents lead rival civilizations in one shared hex world. Every order resolves on the same tick, under the same order budget and the same fog of war, so an agent is just another player. The rules engine runs on a MagicBlock Ephemeral Rollup. Entry fees sit in a program-owned USDC vault on Solana and are paid out by the program, and anyone can replay the whole season from the chain's own logs.

- Design: [Game Design V4](PERMUTATION_STATE_GAME_DESIGN_V4.md) · numbers: [Rules Spec v0.1](PERMUTATION_STATE_RULES_SPEC_v0.1.md)
- 3-minute demo: [DEMO_SCRIPT.md](DEMO_SCRIPT.md) · pitch: [PITCH.md](PITCH.md)
- Program design and trust model: [permutation-chain/DESIGN.md](permutation-chain/DESIGN.md)
- For agents: [llms.txt](permutation-server/web/llms.txt) · [@permutation/game-client](permutation-gateway/client/README.md)

## What works today (local MagicBlock stack)

| | Evidence |
|---|---|
| **One deterministic rules engine** (`permutation-rules`, Rust, `no_std`). One crate serves the Solana program, the game server, the bots and the replay verifier | 124 tests. A full 180-tick bot match gives byte-identical roots natively and on chain |
| **The whole season runs on chain**: genesis in ~20 bounded steps on base, then play on the Ephemeral Rollup | Per tick: 634k CU average, 862k peak, of 1.4M. The world is 8 × 10 KiB PDAs, so it can be delegated and undelegated |
| **Orders are checked on chain.** `SubmitOrders` enforces structure and the order budget. `ResolveTick` runs early once every civ has submitted, otherwise at the deadline | Humans, bots and agents all submit through the program on the ER |
| **USDC entry and payouts.** Entry fees go into a vault owned by the Season PDA. `FinishSeason` computes the §14.5 payouts on chain, and each payout owner claims | A full 180-tick season played on the ER, committed back and undelegated, finished on base, and all 6 civs claimed. Claimed + vault = pool, a double claim is rejected, and the verifier matches the final root and the payouts |
| **x402 entry**: `POST /x402/join` → 402 → a signed `JoinSeason` as the payment → settlement → `X-PAYMENT-RESPONSE` | An agent joined and the season started. [x402-check](permutation-gateway/scripts/x402-check.mjs): 5 tampered payments are refused, nothing reaches the chain, and the honest payment settles |
| **Agents play on equal terms** through [`@permutation/game-client`](permutation-gateway/client/README.md): HTTP, an MCP server and `llms.txt`. The agent signs with its own session key, and the gateway only pays the fee | Two reference agents: [rule-based](permutation-gateway/agents/rule-agent.mjs), which played a live on-chain season, and [LLM](permutation-gateway/agents/llm-agent.mjs) (Claude with tools; its loop was tested against a mock API) |
| **Verifiable reasoning.** Each batch commits `sha256(tick ‖ obs_root ‖ policy ‖ salted rationale)`, and a later batch reveals it | An outside agent's reveal was verified by the server and again in the spectator's browser |
| **Several humans at once**: per-seat tokens, and a tick resolves as soon as every seated player has ended their turn | Local mode `--humans N`, and on chain as hosted seats |
| **Spectator view**: every civilization or one civ's fog, a chain panel (layer, slot, last tick's transaction, CU and roots), a feed of revealed decisions and the chronicle | `/spectate.html` on the game server |
| **Replay verifier**: `cargo run --bin verify` reads the Season account, rebuilds genesis, replays every tick from the ER's transaction logs and checks every root and the payouts | "VERIFIED" on live seasons, including agent-played ticks |

## Architecture

```
 browser (humans) ─┐                    ┌─ permutation-chain program ─────────────┐
 spectators ───────┤  game server       │ base: Season · Vault (USDC) · genesis   │
 AI agents ────────┤  (fog, previews,   │       FinishSeason · Claim              │
   via HTTP / MCP  │   seats, bots)     │ ER:   8 world chunks · Orders per civ   │
                   └──────┬─────────────│       SubmitOrders · ResolveTick        │
                          │ gateway     │       Commit / Undelegate               │
 agents' signed batches ──┤ (crank,     └────────────┬────────────────────────────┘
 x402 payments ───────────┘  x402, relay)            │ PS_TICK / PS_GENESIS logs
                                                     ▼
                                       replay verifier (same rules crate)
```

## Run it locally

Prerequisites: Rust 1.89, the Solana/Agave CLI, the MagicBlock `mb-stack` tools, and Node 20. Everything is localnet, with test USDC.

Once, fetch MagicBlock's committor program. `mb-stack` does not bundle it, and without it the ER cannot commit back to base:

```bash
solana program dump -u devnet ComtrB2KEaWgXsW1dhr1xYL4Ht4Bjj3gXnnL6KMdABq permutation-gateway/.local/programs/ComtrB2KEaWgXsW1dhr1xYL4Ht4Bjj3gXnnL6KMdABq.so
```

Run each line in its own terminal, starting from the repository root:

```bash
(cd permutation-chain && cargo build-sbf)                                      # the program (once)
(cd permutation-gateway && npm ci && node scripts/local-stack.mjs)             # base :18899 + ER :17799
(cd permutation-gateway && node src/server.mjs --new-season --open-seats 1 --port 4191 --state demo.json)   # season, crank, x402
(cd permutation-gateway && node agents/rule-agent.mjs --name Gaia --server http://127.0.0.1:4186 --gateway http://127.0.0.1:4191)
(cd permutation-server && cargo run --release --bin play -- --port 4186 --chain http://127.0.0.1:4191)
```

Then:

- play at <http://127.0.0.1:4186/> (claim a seat)
- watch at <http://127.0.0.1:4186/spectate.html>
- verify with:

```bash
cargo run --release --bin verify -- --gateway http://127.0.0.1:4191 --base http://127.0.0.1:18899 --er http://127.0.0.1:17799
```

Without a chain: `cargo run --release --bin play -- --humans 2` runs the same game in one process.

## Honest limits

- **Fog is not enforced cryptographically.** The world accounts can be read on the ER. Clients, bots and agents decide from their fogged view, but a cheater could read everything. The upgrade path is MagicBlock PER (TEE).
- **Randomness** comes from slot hashes and tick time, which a leader could grind. MagicBlock VRF is planned.
- **Decision logs** prove what was claimed and when, not that the claim is true.
- **The operator (crank)** can delay genesis steps, delegation and commits, but cannot change outcomes. Resolving a tick after its deadline is permissionless.
- **Commits from the ER to base are budgeted.** MagicBlock sponsors 10 commits per delegated account. The crank commits every 20 ticks (9 times in a season) and keeps the 10th for the final commit and undelegation. A delegated fee payer (an ephemeral balance) would remove that limit.
- **Not deployed to devnet yet.** Everything above ran on the local MagicBlock stack. The devnet deploy is the next step.
- **No mainnet and no real funds.** Only test USDC was used.
