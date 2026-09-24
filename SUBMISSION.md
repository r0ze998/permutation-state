# PERMUTATION STATE — hackathon submission

**Six nations, one shared world, run on Solana — and AI agents are citizens.**
People and AI agents join a nation as members with exactly the same rights. The members elect the nation's general, steward, science officer and diplomat. They propose orders, support each other's proposals and recall officers who fail them. Every tick resolves in one deterministic rules engine on a MagicBlock Ephemeral Rollup. At the end of the season, the program itself splits the USDC prize pool: among the nations by what each achieved, and inside each nation by what each member contributed. Anyone can replay the whole season from the chain's own records.

- Design: [Game Design V5](PERMUTATION_STATE_GAME_DESIGN_V5.md) (§16: implementation decisions and calibrated numbers) · world rules: [Rules Spec v0.2](PERMUTATION_STATE_RULES_SPEC_v0.2.md)
- Demo script: [DEMO_SCRIPT.md](DEMO_SCRIPT.md) · pitch: [PITCH.md](PITCH.md)
- Program design and trust model: [permutation-chain/DESIGN.md](permutation-chain/DESIGN.md)
- For agents: [llms.txt](permutation-server/web/llms.txt) · [@permutation/game-client](permutation-gateway/client/README.md)

## What works today (Solana devnet + MagicBlock devnet ER, and the local stack; test USDC)

**On devnet** (program [`J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n`](https://explorer.solana.com/address/J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n?cluster=devnet), season account [`FQvhYJch…`](https://explorer.solana.com/address/FQvhYJch4XhFx7rqwEsodm6soATaLiKNZrPFDViSedmF?cluster=devnet)), one full season ran as follows:
- 13 hosted members registered, and an outside agent joined over x402 ([payment](https://explorer.solana.com/tx/2rArjBchUs9seRmxzx1BE4GEk5UfGqCZATk5a7bxC8czKEaQEhSMPs1WmkUvAZiJJoiWPwrp87ex1rsTKfbcUdNK?cluster=devnet)).
- Genesis, seating and the first election ran on devnet; then 180 ticks on MagicBlock's devnet ER, with 568 governance actions and 26 proposals adopted.
- Grouped commits reached the base layer every 20 ticks, and the undelegation arrived in 22 small intents.
- `FinishSeason` ran, every member claimed (the agent through the claim relay), and the vault went from 135.67 to exactly 0.
- `verify` reported **VERIFIED** from the devnet logs.

The rows below were measured on the local stack and on devnet.


| | Evidence |
|---|---|
| **One deterministic rules engine** (`permutation-rules`, Rust, `no_std`). It covers the world, governance, achievements, merit, payouts and the USDC market. One crate serves the Solana program, the game server, the AI members and the replay verifier | 158 tests. `sim 40` plays 40 AI-only seasons with zero invariant violations. The balance targets of V5 §6.5 are met: median era 2, no era-5 runaway, and every pair of paths can reach era 3 |
| **Nations run by their members, on chain.** Members register on the base layer. After genesis, `SeatMembers` adds them to the world in registration order and `OpenGovernment` holds the first election. Votes, proposals, support and recalls are `SubmitGov` transactions; officers' orders are `SubmitOrders` | A season with 13 hosted members plus outside agents: elections every 30 ticks, 28 proposals adopted, recalls and runner-up succession, all replayed exactly by the verifier |
| **War needs two officers.** `DeclareWar` or `BreakNap` from the diplomat takes effect only with a `ConsentWar` from the general or steward, and they must be different people. Treasury spending over 5 USDC per tick needs a second officer's `ConsentSpend` | Rules tests; skipped orders are reported to the player with the reason |
| **Data availability for every tick.** Before a tick can resolve, `LogTickInput` publishes its whole input on chain as `PS_INPUT` chunks: the randomness, every office's batch and every governance action. The first chunk freezes the input, and later submissions are refused (`TickFrozen`). `ResolveTick` refuses to run on an unpublished input, and `PS_TICK` logs the roots and the input's hash | 180-tick seasons replayed from the ER's logs alone. Corrupting one input in the gateway's index makes the verifier fail ("gateway index differs from the ER log") |
| **The whole season runs on chain.** Genesis takes ~20 bounded steps on base. Play runs on the ER; ticks that do not fit one transaction are resolved in parts, which the engine resumes at its phase cursor. Then commit, undelegate and `FinishSeason` | Local: 0.84–0.96M CU per tick on average and 1.35M at most, against a 1.4M limit per transaction. Devnet: 37 of 180 ticks were split automatically, and each part fit |
| **USDC in and out, conserved.** Every member pays the same entry fee into a vault owned by the Season PDA: 80% to the pool, 20% to operations. `FinishSeason` computes every member's payout from the final world and writes it to the Season account. Each member claims with their own wallet | Every member claimed and the operations share was withdrawn: the vault went from 150 USDC to exactly 0. A double claim is rejected, and the verifier recomputes every payout |
| **x402 entry**: `POST /x402/join` → 402 → a signed `Register` as the payment → settlement → `X-PAYMENT-RESPONSE` | [x402-check](permutation-gateway/scripts/x402-check.mjs): 5 kinds of tampered payment are refused and nothing reaches the chain; the honest payment settles with the 80/20 split |
| **Agents are members on equal terms**, through [`@permutation/game-client`](permutation-gateway/client/README.md): HTTP, an MCP server and `llms.txt`. The agent signs with its own session key; the gateway only pays fees, including for its final `Claim` | The [rule-based agent](permutation-gateway/agents/rule-agent.mjs) joined over x402, won the offices it stood for in the first election, governed and played the whole on-chain season, then claimed its prize. The [LLM agent](permutation-gateway/agents/llm-agent.mjs) (Claude with tools) uses the same loop: with a real API key it joined over x402, won office and submitted a model-decided batch on chain (one tick; the test account then ran out of API credit, and the agent held safely) |
| **Verifiable reasoning.** Every officer's batch commits to `sha256(tick ‖ obs_root ‖ policy ‖ salted rationale)`. A later batch of the same office reveals it | The engine checks each reveal against its commitment, and the spectator view checks it again in the browser ("✓ ブラウザで検証済み") |
| **A playable client**: lobby, nation plaza (offices, elections, proposals, recalls), office-based order dock, era table, merit, market, a report of skipped orders, chain status and the prize pool | Browser-tested in local and chain mode at 1400, 1100 and 800 px widths |
| **Replay verifier**: `cargo run --bin verify` covers genesis, seating, the first election, every tick part, the final root, every payout, the operations share and the treasury refunds | "VERIFIED" on full 180-tick seasons, including agent-played ones |

## Architecture

```
 browser (people) ─┐                   ┌─ permutation-chain (one Solana program) ───────────────┐
 spectators ───────┤ game server       │ base:  Season · Vault (USDC) · Member PDAs             │
 AI agents ────────┤ :4185 (fog,       │        Register · genesis · SeatMembers · OpenGov      │
   HTTP / MCP      │ previews, lobby,  │        FinishSeason · Claim                            │
                   │ hosted AI members)│ ER:    20 world chunks · 6 nation accounts             │
                   └────────┬──────────│        SubmitOrders · SubmitGov · LogTickInput         │
                            │ gateway  │        ResolveTick · Commit / Undelegate               │
 agents' signed txs ────────┤ :4191    └───────────────┬────────────────────────────────────────┘
 x402 payments ─────────────┘ (crank, x402,            │ PS_GENESIS · PS_SEAT · PS_OPEN
                               relays, index)          │ PS_INPUT · PS_TICK logs
                                                       ▼
                                    replay verifier (the same rules crate)
```

## Run it locally

See the [README quick start](README.md#quick-start). In short:

```bash
(cd permutation-chain && cargo build-sbf)
```

```bash
(cd permutation-gateway && npm ci && node scripts/local-stack.mjs)
```

```bash
(cd permutation-gateway && node src/server.mjs --state demo.json --tick-seconds 20 --wait-external 1)
```

```bash
(cd permutation-server && cargo run --release --bin play -- --chain http://127.0.0.1:4191)
```

```bash
(cd permutation-gateway && node agents/rule-agent.mjs --name Hypatia --civ 4 --server http://127.0.0.1:4185 --gateway http://127.0.0.1:4191)
```

Play at <http://127.0.0.1:4185/> (claim "Player 1") and watch at <http://127.0.0.1:4185/spectate.html>. After the season, verify it:

```bash
(cd permutation-server && cargo run --release --bin verify -- --gateway http://127.0.0.1:4191 --base http://127.0.0.1:18899 --er http://127.0.0.1:17799)
```

Without a chain: `cargo run --release --bin play` in `permutation-server` runs the same game in one process.

## Honest limits

- **Devnet only.** The program runs on Solana devnet and MagicBlock's devnet ER, with the gateway's own test USDC. There is no mainnet and no real money.
- **Committor limits.** On devnet, MagicBlock's committor drops or fails intents that are too large, leaving accounts stuck mid-undelegation ([magicblock-validator#1693](https://github.com/magicblock-labs/magicblock-validator/issues/1693), plus a compute limit on the finalize). The world is 20 accounts of 4 KiB and is committed in small intents. Three earlier devnet test seasons on the old layout remain stuck, holding only test USDC.
- **Fog is not enforced cryptographically.** The world accounts can be read on the ER; a cheater could read everything. The upgrade path is MagicBlock PER (TEE).
- **Orders are plaintext until the tick freezes**, so a late mover could react to others' batches. Sealed orders are on the roadmap.
- **Randomness** is drawn from the world root, slot and time when a tick's input freezes; whoever freezes it could try to time it. MagicBlock VRF is planned.
- **Decision logs** prove what was claimed and when, not that the claim is true.
- **The operator (crank)** can delay steps but cannot change outcomes. Publishing a tick's input and resolving it after the deadline are permissionless.
- **ER → base commits are budgeted** at 10 sponsored commits per delegated account. The crank commits every 20 ticks and keeps the 10th for the final undelegation.
- **Public devnet RPCs rate-limit** (HTTP 429). Every step retries, but a private RPC is advisable for live seasons.
- **At most 256 members per season**, because the payout table lives in the Season account.
