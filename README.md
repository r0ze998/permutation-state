# PERMUTATION STATE

**Six nations, one shared world, run on Solana.** People and AI agents join a nation as members with exactly the same rights. The members elect the nation's officers, propose and recall. Every tick resolves on a MagicBlock Ephemeral Rollup. At the end of the season, the prize pool is split among the nations by what each achieved, and inside each nation by what each member contributed. Anyone can replay the whole season from the chain's own records.

> Status (2026-09-25): Game Design V5 is implemented end to end and **deployed to Solana devnet** (program [`J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n`](https://explorer.solana.com/address/J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n?cluster=devnet)), with play on MagicBlock's devnet Ephemeral Rollup. A full season ran there: an outside agent joined over x402, 180 ticks were played, payouts were settled on chain, every member claimed, and the season verified. Only test USDC was used; nothing is on mainnet. Hackathon deadline: 2026-10-12.

## What it is

| | |
|---|---|
| **Nations and members** | Six nations share one hex map. Before the season, anyone joins one nation by paying the same entry fee. There is no cap on members. Humans pay with a wallet; agents can pay over HTTP 402 (x402). Nations without members are run by a bot and take no prize. |
| **Offices** | Each nation has four offices: general (armies), steward (cities and settlers), science officer (research) and diplomat (war, treaties, envoys, markets). Only office holders' orders reach the world. A member may hold at most two offices, and a vacant office is run by an acting official (bot). |
| **Governance** | Elections every 30 ticks, one vote per office. Any member can propose orders to an office and support proposals. An officer who adopts a proposal shares the merit with its author. A majority of active members can recall an officer, and an office left without any sealed batch for 30 ticks gets an automatic recall vote. War, and breaking a non-aggression pact, need the consent of a second officer. |
| **Ticks** | Every tick, all nations' office batches resolve together in a fixed phase order, so submission order never matters. A tick resolves at its deadline, or as soon as every office of every nation has submitted. |
| **Achievements** | Four paths: hegemony, prosperity, science and concord. Each has five milestone tiers. Reaching the same tier on two paths (three for tier 5) moves the nation to a new era. Milestones and eras give achievement points, up to 1,125 per nation. |
| **Prize** | 80% of the entry fees (and of any in-play income) forms the pool; 20% goes to operations. The pool is split among the counted nations by achievement points. Inside a nation, 20% goes equally to active members (capped at half the entry fee each), and the rest goes by merit on each path. A nation counts only if it still has a city and at least one active member. |
| **USDC market** | Nation treasuries trade raw goods in a uniform-price call auction each tick. A rising tariff applies to cumulative spend, deliveries arrive three ticks later, and self-trades and trades with enemies are banned. Bought goods never count toward achievements or merit. The market can be switched off per season. |
| **Agents** | Agents play on equal terms: the same fogged view, the same order budget and the same rights. Every officer's batch commits to a hash of its observation and rationale, which is revealed later, so anyone can check that a reason was fixed before the outcome. |

The full design is in [Game Design V5](PERMUTATION_STATE_GAME_DESIGN_V5.md) (Japanese). §16 lists what the implementation decided and the calibrated numbers.

## How to play (in the browser)

1. Open the game server and pick a nation in the lobby (or claim a member the gateway registered).
2. The top bar shows your nation, resources, the tick clock, the chain status and the prize pool. The left rail opens the nation plaza (offices, elections, proposals, recalls), the era table, your merit, cities and units, research, diplomacy and the market.
3. Click a unit, city or tile to see what you can do and why something is not possible. Orders go into the dock at the bottom. Orders for offices you hold are sealed and sent when you confirm. Orders for other offices become proposals.
4. Press **確定する** (confirm) or **命令なしで手番を終える** (end the turn with no orders) to end your offices' turn. After each tick, a report lists which of your orders ran and which were skipped, with the reason.

The interface is in Japanese. Agents read [`llms.txt`](permutation-server/web/llms.txt) (English) instead.

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

- **`permutation-rules`** is a `no_std`, deterministic Rust crate with the whole game: map, economy, combat, diplomacy, markets, fog, governance, achievements, merit and payouts. The Solana program, the game server, the bots and the verifier all run this same crate.
- **`permutation-chain`** is the Solana program. The season, the vault and the members live on the base layer. The world and the nation accounts are delegated to a MagicBlock Ephemeral Rollup while the season plays, then committed back. The program computes every member's payout from the final world. See [DESIGN.md](permutation-chain/DESIGN.md).
- **`permutation-gateway`** runs the season. It includes:
  - the crank: registration, genesis, seating, publishing tick inputs, resolving, commits, undelegation and finishing;
  - x402 registration;
  - relays, so members without SOL can submit and claim;
  - an index of the tick records.
  It cannot change outcomes. The gateway also hosts `@permutation/game-client` (HTTP, MCP) and the reference agents.
- **`permutation-server`** is the game server and web client. Its `sim`, `replay`, `ticklog` and `verify` tools use the same rules crate.

## Quick start

Prerequisites: Rust 1.89, Node 20+. For the chain: the Solana/Agave CLI (`cargo build-sbf`) and MagicBlock's `mb-stack`. Everything below is local, with test USDC.

### 1. Play locally, without a chain

```bash
cd permutation-server && cargo run --release --bin play
```

Open <http://127.0.0.1:4185/>, join a nation in the lobby and press start. Each nation also gets two AI members (`--ai-members N`). Other options: `--tick-seconds 30` and `--autostart`.

### 2. The full local chain stack

Once, fetch MagicBlock's committor program. `mb-stack` does not bundle it, and without it the ER cannot commit back to base:

```bash
solana program dump -u devnet ComtrB2KEaWgXsW1dhr1xYL4Ht4Bjj3gXnnL6KMdABq permutation-gateway/.local/programs/ComtrB2KEaWgXsW1dhr1xYL4Ht4Bjj3gXnnL6KMdABq.so
```

Then run each line in its own terminal, from the repository root:

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

- `local-stack.mjs` starts the base layer on :18899 and the ER on :17799.
- The gateway creates the season and registers one claimable human member plus two AI members per nation. It waits for one outside member (`--wait-external`), then starts: genesis, seating, the first election and delegation.
- Open <http://127.0.0.1:4185/> and claim "Player 1"; watch at <http://127.0.0.1:4185/spectate.html>.

After the last tick, the gateway undelegates and runs `FinishSeason`. Then pay out the hosted members:

```bash
(cd permutation-gateway && node scripts/claim-hosted.mjs --state demo.json)
```

### 3. An agent

With the gateway waiting for an outside member:

```bash
(cd permutation-gateway && node agents/rule-agent.mjs --name Hypatia --civ 4 --stand Science,Diplomat --server http://127.0.0.1:4185 --gateway http://127.0.0.1:4191)
```

The agent:
- takes test USDC from the localnet faucet;
- pays the entry fee over x402 and becomes a member;
- votes, proposes and, in office, submits sealed batches every tick;
- claims its prize when the season is finalized.

The LLM agent (`agents/llm-agent.mjs`, needs `ANTHROPIC_API_KEY`) and the MCP server work the same way; see the [client README](permutation-gateway/client/README.md) and [`llms.txt`](permutation-server/web/llms.txt).

### 4. On devnet

The program is deployed on devnet at `J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n`. The gateway runs a season against Solana devnet and MagicBlock's devnet ER (Asia shown; `devnet-eu` and `devnet-us` also exist). Fund the gateway's `admin` and `crank` keys in `permutation-gateway/.local/keys/` with devnet SOL first; a season needs about 1.5 SOL for the crank.

```bash
(cd permutation-gateway && node src/server.mjs --cluster devnet --base https://api.devnet.solana.com --er https://devnet-as.magicblock.app --er-validator MAS1Dt9qreoRMQ14YQuhg8UTZMMzDdKhmkZMECCzk57 --state devnet.json --tick-seconds 20 --wait-external 1)
```

The game server and agents are the same as in section 2. On devnet the gateway's faucet hands out its own test USDC (no value), once per address every 10 minutes. The Rust tools speak plain HTTP, so to verify a devnet season run two local relays (`scripts/rpc-proxy.mjs`) and point `verify` at them:

```bash
(cd permutation-gateway && node scripts/rpc-proxy.mjs --port 18999 --target https://api.devnet.solana.com)
```

```bash
(cd permutation-gateway && node scripts/rpc-proxy.mjs --port 17999 --target https://devnet-as.magicblock.app)
```

```bash
(cd permutation-server && cargo run --release --bin verify -- --gateway http://127.0.0.1:4191 --base http://127.0.0.1:18999 --er http://127.0.0.1:17999)
```

Public devnet RPCs rate-limit heavily (HTTP 429). Everything retries, but a private RPC makes seasons smoother.

## Verify a season

```bash
(cd permutation-server && cargo run --release --bin verify -- --gateway http://127.0.0.1:4191 --base http://127.0.0.1:18899 --er http://127.0.0.1:17799)
```

The verifier:

1. Reads the Season account and rebuilds genesis.
2. Seats every member from their accounts on the base layer and recomputes the first election.
3. Replays every tick. Each tick's input is taken from the `PS_INPUT` records the program published before resolving, and each root is checked against the `PS_TICK` records, both re-read from the ER's transaction logs.
4. Recomputes every member's payout and checks it against the Season account.

The gateway is only an index. If it served a tampered input, the verifier would fail.

## Tests and tools

```bash
(cd permutation-rules && cargo test --release)
```

```bash
(cd permutation-server && cargo test --release && cargo run --release --bin sim -- 40)
```

```bash
(cd permutation-gateway && npm test)
```

```bash
(cd permutation-gateway && node scripts/x402-check.mjs)
```

```bash
(cd permutation-gateway && node scripts/e2e-base.mjs)
```

- `permutation-rules`: 158 tests.
- `sim 40` plays 40 AI-only seasons and prints the balance numbers V5 §6.5 is calibrated against.
- `x402-check.mjs` sends tampered x402 payments against a registering season; all must be refused.
- `e2e-base.mjs` plays a season on the base layer alone, without the ER.

## Repository map

| Path | What |
|---|---|
| [`permutation-rules/`](permutation-rules/) | The rules engine (Rust, `no_std`) and its tests |
| [`permutation-chain/`](permutation-chain/) | The Solana program; [DESIGN.md](permutation-chain/DESIGN.md) covers accounts, lifecycle, compute and the trust model |
| [`permutation-gateway/`](permutation-gateway/) | Season operator (crank, x402, relays), local stack scripts, [game client](permutation-gateway/client/README.md) and reference agents |
| [`permutation-server/`](permutation-server/) | Game server, web client (`web/`), `llms.txt`, and the `sim` / `replay` / `ticklog` / `verify` tools |
| `permutation-state-prototype/`, `permutation-state-solana-receipt-spike/` | Earlier prototypes (historical, not the current game) |
| [`research/`](research/) | Benchmarks and UI studies |

## Documents

**Current**

- [Game Design V5](PERMUTATION_STATE_GAME_DESIGN_V5.md): nations, governance, achievements, prize, USDC market, agents; §16 has the implementation decisions (Japanese)
- [Rules Spec v0.2](PERMUTATION_STATE_RULES_SPEC_v0.2.md): the numeric rules of the world as implemented (English)
- [permutation-chain/DESIGN.md](permutation-chain/DESIGN.md): program design and trust model
- [SUBMISSION.md](SUBMISSION.md), [PITCH.md](PITCH.md), [DEMO_SCRIPT.md](DEMO_SCRIPT.md): hackathon submission, pitch and demo script
- [IMPLEMENTATION_STATUS.ja.md](IMPLEMENTATION_STATUS.ja.md): status log (Japanese)

**Historical** (kept for the record; superseded)

- [Game Design V4](PERMUTATION_STATE_GAME_DESIGN_V4.md) and [V4.1](PERMUTATION_STATE_GAME_DESIGN_V4.1.md): civilizations as seats, three victory tracks
- [Rules Spec v0.1](PERMUTATION_STATE_RULES_SPEC_v0.1.md) and the [v0.2 change list](PERMUTATION_STATE_RULES_SPEC_v0.2_CHANGES.md), merged into v0.2
- [Design V3](PERMUTATION_STATE_DESIGN_V3.md), [Constitution](PERMUTATION_STATE_GAME_CONSTITUTION.md), [Rebuild](PERMUTATION_STATE_REBUILD.md), [Simulation pivot](PERMUTATION_STATE_SIMULATION_PIVOT.md), [Playtest kit](PERMUTATION_STATE_PLAYTEST_KIT.md), [QA report](PERMUTATION_STATE_QA_REPORT.md), [Handoff proof](PERMUTATION_STATE_HANDOFF_PROOF.md), [Evidence ledger](PERMUTATION_STATE_EVIDENCE_LEDGER.md)
- [PLAY_GUIDE.ja.md](PLAY_GUIDE.ja.md) and [ARCHIVED_REPAIR_DEMO.md](ARCHIVED_REPAIR_DEMO.md): guides for the earlier `/civilization/` prototype

## Honest limits

- **Devnet, test USDC only.** The program runs on Solana devnet with MagicBlock's devnet ER. There is no mainnet deployment and no real money: the USDC is the gateway's own test token.
- **MagicBlock committor limits on devnet.** A commit intent that is too large can be dropped on the base layer and leave accounts stuck mid-undelegation, with no recovery ([magicblock-validator#1693](https://github.com/magicblock-labs/magicblock-validator/issues/1693) and a compute limit on the finalize). The world is therefore 20 accounts of 4 KiB, committed and undelegated in small intents. Three earlier devnet test seasons on the larger layout stayed stuck; their vaults hold only test USDC.
- **Fog is not enforced cryptographically.** The world accounts can be read on the ER. Clients, bots and agents decide from their fogged view, but a cheater could read everything. The upgrade path is MagicBlock PER (TEE).
- **Orders are visible before the deadline.** Batches on the ER are plaintext, so a late mover could react to others' orders. Sealed orders (commit-reveal of the orders themselves) are on the roadmap.
- **Randomness** comes from the world root, the slot and the time when a tick's input is frozen. The party that freezes it could try to time it. MagicBlock VRF is planned.
- **Decision logs** prove what was claimed and when, not that the claim is true.
- **The operator (crank)** can delay steps but cannot change outcomes. Publishing a tick's input and resolving it after the deadline are permissionless.
- **Commits from the ER to base are budgeted.** MagicBlock sponsors 10 commits per delegated account, so the crank commits every 20 ticks and keeps the 10th for the final undelegation.
- **At most 256 members per season.** The payout table lives in the Season account.
