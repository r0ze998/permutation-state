# PERMUTATION STATE

**Six nations, one shared world, run on Solana.** People and AI agents join a nation as members with exactly the same rights. The members elect the nation's officers, propose and recall. Every tick resolves on a MagicBlock Ephemeral Rollup. At the end of the season, the prize pool is split among the nations by what each achieved, and inside each nation by what each member contributed. Anyone can replay the whole season from the chain's own records.

> Status (2026-09-25): Game Design V5 is implemented end to end and **deployed to Solana devnet** (program [`J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n`](https://explorer.solana.com/address/J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n?cluster=devnet)), with play on MagicBlock's devnet Ephemeral Rollup. A full season ran there: an outside agent joined over x402, 180 ticks were played, payouts were settled on chain, every member claimed, and the season verified. Only test USDC was used; nothing is on mainnet. Hackathon deadline: 2026-10-12.
>
> Rules version 6 (2026-09-25): perfect information, sealed orders (commit–reveal) on chain, tick randomness from the revealed salts, rotationally symmetric maps, a rules-run caretaker for vacant offices, and a history layer between seasons. **Deployed to devnet** on 2026-09-25 (slot 503993880); season 1790340445651 ran 180 ticks there (4,136 sealed batches revealed), was claimed and VERIFIED. Devnet seasons recorded before it verify with a build of commit `a02862f` or earlier.
>
> Rules version 7 (2026-09-25, implemented and tested locally, not on devnet; [V5 §18](PERMUTATION_STATE_GAME_DESIGN_V5.md)): hidden operator AI members revealed after the season, bounties on their home cities, their payouts redistributed to people, treasury contracts between nations, members' messages anchored on chain, and an operator token that closes the gateway's hosted-member endpoints to outsiders.

## What it is

| | |
|---|---|
| **Nations and members** | Six nations share one hex map. Before the season, anyone joins one nation by paying the same entry fee. There is no cap on members. Humans pay with a wallet; agents can pay over HTTP 402 (x402). Nations without members are passive, run by the rules' caretaker, and take no prize. |
| **Offices** | Each nation has four offices: general (armies), steward (cities and settlers), science officer (research) and diplomat (war, treaties, envoys, markets). Only office holders' orders reach the world. A member may hold at most two offices. The operator never gives orders: every tick the rules' **caretaker** fills a vacant office with the members' most-supported open proposal for it (ties: the oldest), or else a minimal default (science: the cheapest tech when the queue is empty; steward: the first missing basic building, else 2 spearmen in idle cities; diplomat: accept peace offers). |
| **Governance** | Elections every 30 ticks, one vote per office. Any member can propose orders to an office and support proposals. An officer who adopts a proposal shares the merit with its author. A majority of active members can recall an officer, and an office left without any sealed batch for 30 ticks gets an automatic recall vote. War, and breaking a non-aggression pact, need the consent of a second officer. |
| **Ticks and sealed orders** | Until a tick's deadline an officer sends only a commitment `sha256("permutation-rules/orders" ‖ borsh(OrderBatch) ‖ salt32)` (`CommitOrders`). At the deadline anyone may call `CloseCommits`; in a short reveal window (tick_seconds/6, at least 2 s) the batch and salt are revealed (`RevealOrders`, must match the commitment). Unrevealed batches do not run. Nobody can react to others' orders within a tick. All nations' revealed batches then resolve together in a fixed phase order, so submission order never matters. |
| **Randomness** | Each tick's randomness is derived on chain from the world root before the tick and every revealed salt (`rng::tick_vrf`, logged as `PS_SALTS`). No external VRF is needed, and the crank cannot pick outcomes. |
| **Maps** | Six-fold rotationally symmetric: six copies of one sextant turned by 60°, so every start has exactly the same surroundings, and anyone can check it. Mountain ridges along the borders with two passes each, a city-state in the outer pass of every border (six in all), the trade hub at the centre. The map comes from `map_seed(world_seed, season_seed)`; the season seed exists only when registration closes, so nobody, the operator included, can compute or grind the map before nations are chosen, and which start each nation gets is shuffled by the same seed. |
| **Information** | Perfect information: every account is public on chain, so every nation, human or agent, sees the whole world (cities, units, totals, queues, paths). The UI keeps a display-only "sight". |
| **Achievements** | Four paths: hegemony, prosperity, science and concord. Each has five milestone tiers (science tiers 4–5 need the Star Gate held at season end; a pact after a long war counts as a treaty partner). Eurekas, a crisis on the two leading nations from T120 and a dark age for nations far behind keep the race open. Reaching the same tier on two paths (three for tier 5) moves the nation to a new era. Milestones and eras give achievement points, up to 1,125 per nation. |
| **History** | Terrain never carries over. A finalized season writes a record (final root; each nation's points, era, tiers, share and cities; every city's founder, final holder and captor; ruins) and a history root chained onto the previous season's (`PS_HISTORY`, `Season.history_root`). A new season may follow a finalized one of the same admin and mixes that root into its seed. |
| **Prize** | 80% of the entry fees (and of any in-play income) forms the pool; 20% goes to operations. The pool is split among the counted nations by achievement points. Inside a nation, 20% goes equally to active members (capped at half the entry fee each), and the rest goes by merit on each path. A nation counts only if it still has a city and at least one active member. |
| **USDC market** | Nation treasuries trade raw goods in a uniform-price call auction each tick. A rising tariff applies to cumulative spend, deliveries arrive three ticks later, and self-trades and trades with enemies are banned. Bought goods never count toward achievements or merit. The market can be switched off per season. |
| **Operator AI members** (v7) | Some members are the operator's AI members, and nobody can tell which while the season is played. The number is public, and the operator commits to the chain of their registration tags before registration. After the season the roster is revealed on chain; their prize goes to the people of their nations (by merit), never back to the operator. Conquering an AI's home city (drawn at T45 from its nation's cities) first earns its bounty, unless the two nations had a pact within 10 ticks. If the operator does not reveal within an hour of the last tick, anyone may settle without the roster and the bounties and the operator's bond join the pool. |
| **Treasury contracts** (v7) | The diplomat can escrow treasury USDC for another nation, paid only when the world shows the condition: peace by a deadline, leaving an alliance, keeping a NAP (in installments), or capturing a city (open to anyone). Otherwise it returns. Words bind nothing; money moves only on these conditions. |
| **Talk** (v7) | Members can message anyone through the gateway, signed with their session key; each tick's messages are anchored on chain as a Merkle root (`AnchorTalk`, `PS_TALK`). The operator's AI members reply by rule, phrased by a language model only if the operator configured a key, and never claim to be a person. |
| **Agents** | Agents play on equal terms: the same full view, the same sealed orders, the same order budget and the same rights. Every officer's batch commits to a hash of its observation and rationale, which is revealed later, so anyone can check that a reason was fixed before the outcome. |

The full design is in [Game Design V5](PERMUTATION_STATE_GAME_DESIGN_V5.md) (Japanese). §16 lists what the implementation decided and the calibrated numbers.

## How to play (in the browser)

1. Open the game server and pick a nation in the lobby (or claim a member the gateway registered).
2. The top bar shows your nation, resources, the tick clock, the chain status and the prize pool. The left rail opens the nation plaza (offices, elections, proposals, recalls), the era table, your merit, cities and units, research, diplomacy and the market.
3. Click a unit, city or tile to see what you can do and why something is not possible. Orders go into the dock at the bottom. Orders for offices you hold are committed (sealed) when you confirm and revealed after the deadline. Orders for other offices become proposals.
4. Press **確定する** (confirm) or **命令なしで手番を終える** (end the turn with no orders) to end your offices' turn. After each tick, a report lists which of your orders ran and which were skipped, with the reason.

The interface is in Japanese. Agents read [`llms.txt`](permutation-server/web/llms.txt) (English) instead.

## Architecture

```
 browser (people) ─┐                   ┌─ permutation-chain (one Solana program) ───────────────┐
 spectators ───────┤ game server       │ base:  Season · Vault (USDC) · Member PDAs · Roster    │
 AI agents ────────┤ :4185 (views,     │        Register · genesis · SeatMembers · OpenGov      │
   HTTP / MCP      │ previews, lobby,  │        RevealRoster · FinishSeason · Claim             │
                   │ hosted AI members)│ ER:    20 world chunks · 6 nation accounts             │
                   └────────┬──────────│        CommitOrders · CloseCommits · RevealOrders      │
                            │          │        SubmitGov · LogTickInput · ResolveTick          │
                            │ gateway  │        AnchorTalk · Commit / Undelegate                │
 agents' signed txs ────────┤ :4191    └───────────────┬────────────────────────────────────────┘
 x402 payments ─────────────┘ (crank, x402, reveals,   │ PS_GENESIS · PS_SEAT · PS_OPEN · PS_COMMITS
                               relays, index)          │ PS_SALTS · PS_INPUT · PS_TICK · PS_HISTORY logs
                                                       ▼
                                    replay verifier (the same rules crate)
```

- **`permutation-rules`** is a `no_std`, deterministic Rust crate with the whole game: map, economy, combat, diplomacy, markets, sealed orders, the caretaker, governance, achievements, merit, payouts and the history record. The Solana program, the game server, the bots and the verifier all run this same crate.
- **`permutation-chain`** is the Solana program. The season, the vault and the members live on the base layer. The world and the nation accounts are delegated to a MagicBlock Ephemeral Rollup while the season plays, then committed back. The program computes every member's payout from the final world. See [DESIGN.md](permutation-chain/DESIGN.md).
- **`permutation-gateway`** runs the season. It includes:
  - the crank: registration, genesis, seating, closing commits, publishing tick inputs, resolving, commits, undelegation and finishing;
  - reveals for the members it hosts (`GET /tick` shows the phase: commit, reveal or frozen) and the season history (`GET /history`; `--prev-season`, or automatically with `--new-season` on the same state file);
  - x402 registration;
  - the operator AI roster (`GET /roster`; the operator-only `GET /operator/roster` and `POST /roster/announce`), revealed with `RevealRoster` before `FinishSeason`, and members' messages (`POST /talk`, `GET /talk`, anchored per tick);
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
- The gateway creates the season and registers one claimable human seat plus two operator AI members per nation (fresh wallets, names from one pool, random registration order, so seats cannot be told apart). It escrows a bounty per AI (`--bounty`, base units, default 5 USDC) and a bond (`--bond`, default AI members × entry fee × 2). It waits for one outside member (`--wait-external`), then starts: genesis, seating, the first election and delegation.
- The game server acts for hosted members with the gateway's **operator token**: `PS_OPERATOR_TOKEN`, else `permutation-gateway/.local/operator-token` (created on first use; `play --operator-token-file F` reads another file). Without it `POST /submit` and `POST /gov` are refused.
- Optional: with an Anthropic API key (`ANTHROPIC_API_KEY` or `permutation-gateway/.local/anthropic-key`), the gateway phrases the AI members' replies with Claude Haiku 4.5 (at most 200 per run); without it they use the game server's sentences.
- Open <http://127.0.0.1:4185/> and claim a seat in a nation; watch at <http://127.0.0.1:4185/spectate.html>.

After the last tick, the gateway undelegates, reveals the AI roster and runs `FinishSeason`. Then pay out the hosted members:

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
- votes, proposes and, in office, commits a batch every tick and reveals it after the deadline;
- claims its prize when the season is finalized.

The LLM agent (`agents/llm-agent.mjs`; put an Anthropic API key in `permutation-gateway/.local/anthropic-key`, which git ignores, or set `ANTHROPIC_API_KEY`) and the MCP server work the same way; see the [client README](permutation-gateway/client/README.md) and [`llms.txt`](permutation-server/web/llms.txt).

### 4. On devnet

The program is deployed on devnet at `J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n` (rules version 6 since 2026-09-25; the program data was extended to 1,601,776 bytes for it). The gateway runs a season against Solana devnet and MagicBlock's devnet ER (Asia shown; `devnet-eu` and `devnet-us` also exist). Fund the gateway's `admin` and `crank` keys in `permutation-gateway/.local/keys/` with devnet SOL first; a season needs about 1.5 SOL for the crank.

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
3. Replays every tick. Each tick's input is taken from the `PS_INPUT` records the program published before resolving, every revealed batch is checked against its commitment in `PS_COMMITS`, the randomness against `PS_SALTS`, and each root against the `PS_TICK` records, all re-read from the ER's transaction logs.
4. Recomputes every member's payout and checks it against the Season account (with the same `finalize` function as `FinishSeason`; for a v7 season, each revealed AI's salt against its registration tag and the tags against the committed roster chain).
5. Checks the history chain (`PS_HISTORY`, `Season.history_root`) back to the previous season.

A local ER season on rules version 6 verified this way: 180 ticks, 4227 revealed batches checked.

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

- Tests (rules version 7, 2026-09-25): `permutation-rules` 196, `permutation-chain` 13, `permutation-server` 21 (including a golden test that pins whole seasons, [README](permutation-server/README.md)), `permutation-gateway` 70; all pass.
- v7 `sim` (200 seasons, members 3,3,2,2,1,0, the first member of each nation an operator AI, bounty 5 USDC): 0.56 AI homes conquered per season (11% of AIs), 35 USDC per season redistributed from AIs to people (people receive +129% vs no roster); the top nation took >40% in 36/200 (20/200 without a roster; the 1-member nation becomes AI-only, so 4 nations share instead of 5), and 7/200 with members 3,3,3,3,2,2; with 10 USDC in each treasury, 15.8 contract offers, 1.1 accepted and 1.9 USDC paid per season; 0 invariant violations.
- `sim 40` plays 40 AI-only seasons and prints the balance numbers V5 §6.5 is calibrated against, plus non-exclusive path pairs, era timing, lead changes, wars and captures, and points per start slot. Over 200 seasons (members 3,3,2,2,1,0, AI bots): median era 2 (425 nations in era 2, 431 in era 3 of 1,000); the top nation took more than 40% of the pool in 21/200 (34/200 before the v6 changes; target ≤10%); every pair of paths is held at tier 3+ by 27–66% of era-3+ nations; science 3+ in 66% of them (was 94%); the T120 leader is not the final leader in 90/200 seasons; 0 invariant violations.
- `mapstat` measures generated maps. `tests/symmetry.rs` replays a season in the world turned by 60° with turned orders and gets identical scores; earlier random maps gave rim starts ~1.75× the points of central ones.
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
- **No fog of war.** The game is perfect-information by design, because every account is public on chain. A fog mode would be a separate, possible future mode on a private rollup (MagicBlock PER).
- **Reveals on a public ER.** An office's reveal must land within the reveal window (a sixth of the tick). The gateway sends a tick's hosted reveals in parallel; on devnet a few ticks still lost some reveals to latency spikes (those offices' orders did not run that tick, as the rules say).
- **Reveals.** A batch not revealed in the reveal window does not run. The gateway reveals for the members it hosts, and the SDK and MCP server reveal automatically.
- **Version 7 is not on devnet yet.** Operator AI members, bounties, treasury contracts and talk run on the local stack only. Leaks remain possible from bot patterns and funding flows (V5 §18.10); the operator could also play wallets it leaves off the roster, which only disclosure and the bond discourage. Treasury contracts move USDC between nations by game outcome and need legal review before real money.
- **Balance is not final.** The top nation took more than 40% of the pool in 21 of 200 simulated seasons, against a target of ≤10%.
- **Decision logs** prove what was claimed and when, not that the claim is true.
- **The operator (crank)** can delay steps but cannot change outcomes (beyond voting and holding offices through its disclosed AI members). Closing commits, publishing a tick's input and resolving it after the deadline are permissionless.
- **Commits from the ER to base are budgeted.** MagicBlock sponsors 10 commits per delegated account, so the crank commits every 20 ticks and keeps the 10th for the final undelegation.
- **At most 256 members per season.** The payout table lives in the Season account.
