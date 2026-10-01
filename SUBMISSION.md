# Wylls — hackathon submission

**Six nations, one shared world, run on Solana — and AI agents are citizens.**
People and AI agents join a nation as members with exactly the same rights. The members elect the nation's general, steward, science officer and diplomat. They propose orders, support each other's proposals and recall officers who fail them. Every tick resolves in one deterministic rules engine on a MagicBlock Ephemeral Rollup. At the end of the season, the program itself splits the USDC prize pool: among the nations by what each achieved, and inside each nation by what each member contributed. Anyone can replay the whole season from the chain's own records.

- Design: [Game Design V5](PERMUTATION_STATE_GAME_DESIGN_V5.md) (§16: implementation decisions and calibrated numbers) · world rules: [Rules Spec v0.2](PERMUTATION_STATE_RULES_SPEC_v0.2.md)
- Demo script: [DEMO_SCRIPT.md](DEMO_SCRIPT.md) · pitch: [PITCH.md](PITCH.md)
- Program design and trust model: [permutation-chain/DESIGN.md](permutation-chain/DESIGN.md)
- For agents: [llms.txt](permutation-server/web/llms.txt) · [@permutation/game-client](permutation-gateway/client/README.md)

**Verifiable fairness (rules version 6, 2026-09-25, local stack; not yet on devnet).** Maps are six-fold rotationally symmetric, so every start has exactly the same surroundings, and the map seed exists only when registration closes. Orders are sealed (commit–reveal on chain), so nobody can react to others' orders within a tick. Each tick's randomness comes from the world root and every revealed salt, so nobody, the crank included, chooses it. The operator never gives orders: the rules' caretaker fills vacant offices from the members' top proposal. Every account is public, so the game is perfect-information for people and agents alike. The devnet season below ran on version 5; it verifies with a build of commit `a02862f` or earlier.

**Hidden AI members, bounties and treasury contracts (rules version 7, 2026-09-25, local stack; not yet on devnet; [V5 §18](PERMUTATION_STATE_GAME_DESIGN_V5.md)).** Some members are the operator's AI members, and nobody can tell which during play. The operator commits to their number and the chain of their registration tags before registration, reveals the roster on chain after the season (`RevealRoster`), and their prize is redistributed to the people of their nations, never back to the operator. Conquering an AI's home city first earns a bounty the operator escrowed. Nations can escrow treasury USDC in contracts paid only when the world shows the condition. Members' messages are anchored on chain every tick. A security fix came with it: the gateway's `POST /submit` and `POST /gov` (acting for hosted members) were callable by anyone; they now require the operator token.

**Wallet membership (2026-09-26, local stack; not yet run on devnet).** People join with their own Solana wallet (Phantom, Solflare or Backpack, through Wallet Standard), take test USDC from the operator's faucet, pay the entry fee over the same x402 route agents use, play with a session key the browser derives from one wallet signature, and claim the prize to their wallet. The wallet signs only the entry and the claim and needs no SOL: the gateway pays every fee. Gateway-hosted human seats are gone. Registration is a fixed window and the season starts at its deadline however many joined. The operator's AI members register at random times within it, with generated names, no declared kind and one neutral policy id, so nothing public tells them apart. The game server serves the gateway's public listener under `/gw`, so one HTTPS origin (a Cloudflare Tunnel or Caddy) puts a season on the internet. A program fix that keeps seating going when two members register the same session key is built and tested, not yet deployed.

## What works today (Solana devnet + MagicBlock devnet ER, and the local stack; test USDC)

**On devnet** (program [`J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n`](https://explorer.solana.com/address/J4aZxe3ynkS7kcvCpKbp6aFYw8d9vtrRDsgSEi1niU6n?cluster=devnet), season account [`FQvhYJch…`](https://explorer.solana.com/address/FQvhYJch4XhFx7rqwEsodm6soATaLiKNZrPFDViSedmF?cluster=devnet)), one full season ran as follows:
- 13 gateway-hosted members registered (before wallet membership), and an outside agent joined over x402 ([payment](https://explorer.solana.com/tx/2rArjBchUs9seRmxzx1BE4GEk5UfGqCZATk5a7bxC8czKEaQEhSMPs1WmkUvAZiJJoiWPwrp87ex1rsTKfbcUdNK?cluster=devnet)).
- Genesis, seating and the first election ran on devnet; then 180 ticks on MagicBlock's devnet ER, with 568 governance actions and 26 proposals adopted.
- Grouped commits reached the base layer every 20 ticks, and the undelegation arrived in 22 small intents.
- `FinishSeason` ran, every member claimed (the agent through the claim relay), and the vault went from 135.67 to exactly 0.
- `verify` reported **VERIFIED** from the devnet logs.

The rows below were measured on the local stack and on devnet.


| | Evidence |
|---|---|
| **One deterministic rules engine** (`permutation-rules`, Rust, `no_std`). It covers the world, governance, achievements, merit, payouts and the USDC market. One crate serves the Solana program, the game server, the AI members and the replay verifier | 196 tests (plus chain 13, server 21, gateway 70; all pass on v7). `sim` over 200 AI-only seasons on v6 (members 3,3,2,2,1,0): median era 2 (425 nations in era 2, 431 in era 3 of 1,000); every pair of paths is held at tier 3+ by 27–66% of era-3+ nations; the T120 leader is not the final leader in 90/200 seasons; zero invariant violations. The top nation took >40% of the pool in 21/200 (was 34/200; target ≤10%) |
| **Nations run by their members, on chain.** Members register on the base layer. After genesis, `SeatMembers` adds them to the world in registration order and `OpenGovernment` holds the first election. Votes, proposals, support and recalls are `SubmitGov` transactions. A vacant office is filled every tick by the rules' caretaker (the members' most-supported open proposal, else a minimal default), never by the operator | A season with 13 hosted members plus outside agents: elections every 30 ticks, 28 proposals adopted, recalls and runner-up succession, all replayed exactly by the verifier |
| **Sealed orders** (v6). Until the deadline an officer sends only `sha256("permutation-rules/orders" ‖ borsh(OrderBatch) ‖ salt32)` (`CommitOrders`). At the deadline anyone may call `CloseCommits` (`PS_COMMITS`); in the reveal window (tick_seconds/6, ≥2 s) `RevealOrders` must match, and unrevealed batches do not run. The gateway reveals its AI members' batches and every batch a browser deposits with it (`POST /seal`, signed by the office's key); the SDK has `submit`, `reveal` and `revealWhenOpen`, and the MCP `submit_orders` reveals in the background | A local ER season on v6 VERIFIED: 180 ticks, 4227 revealed batches checked against `PS_COMMITS` |
| **Randomness nobody chooses** (v6). Each tick's randomness is derived on chain from the world root before the tick and every revealed salt (`rng::tick_vrf`, `PS_SALTS`); no external VRF | The verifier recomputes it from `PS_SALTS` on every tick |
| **Fair maps by construction** (v6). Six copies of one sextant turned by 60°: ridges with two passes on each border, a city-state in every border's outer pass, the trade hub at the centre. `map_seed(world_seed, season_seed)`; starts are shuffled by the season seed, which exists only once registration closes | `tests/symmetry.rs` replays a season in the world turned by 60° with turned orders and gets identical scores. Earlier random maps gave rim starts ~1.75× the points of central ones |
| **Operator AI members, revealed after the season** (v7). `CreateSeason` commits `ai_count` and `roster_chain` and escrows `ai_count × bounty_each + bond`; every `Register` carries a 32-byte tag (an AI's is `sha256("permutation-rules/ai" ‖ season ‖ wallet ‖ salt)`). `RevealRoster` checks each salt against the wallet-signed tag and the chain; `FinishSeason` then pays bounties and redistributes AI payouts to people (the bond returns), or, if not revealed within an hour of the last tick, anyone settles without the roster and the bounties and bond join the pool. AI members get fresh wallets funded through the same faucet as people, names from the one generator everyone uses, and random registration times within the window; in such a season everyone registers the same way, with no declared kind (x402 refuses anything else: `KindHidden`, `UniformRegistration`), every officer commits under the policy id `officer@2`, and neither the gateway's public routes nor the game server's API say who is AI | `sim` over 200 seasons (members 3,3,2,2,1,0, the first member of each nation an AI, bounty 5 USDC): 0.56 AI homes conquered per season (11%); 35 USDC per season redistributed, people receive +129% vs no roster; top nation >40% in 36/200 (20/200 without a roster, since the 1-member nation becomes AI-only; 7/200 with members 3,3,3,3,2,2); 0 invariant violations. The verifier checks every revealed tag and the chain |
| **Treasury contracts** (v7). The diplomat's `OfferContract` / `AcceptContract` / `CancelContract`: peace, leaving an alliance, keeping a NAP (installments), or capturing a city (open offer). Escrow counts as treasury spending (consent over 5 USDC per tick), income cannot be spent on the market, and USDC conservation includes the escrow | Rules tests; `sim` with 10 USDC in each treasury: 15.8 offers, 1.1 accepted, 1.9 USDC paid per season |
| **War needs two officers.** `DeclareWar` or `BreakNap` from the diplomat takes effect only with a `ConsentWar` from the general or steward, and they must be different people. Treasury spending over 5 USDC per tick needs a second officer's `ConsentSpend` | Rules tests; skipped orders are reported to the player with the reason |
| **Data availability for every tick.** Before a tick can resolve, `LogTickInput` publishes its whole input on chain as `PS_INPUT` chunks: the randomness, every office's batch and every governance action. The first chunk freezes the input, and later submissions are refused (`TickFrozen`). `ResolveTick` refuses to run on an unpublished input, and `PS_TICK` logs the roots and the input's hash | 180-tick seasons replayed from the ER's logs alone. Corrupting one input in the gateway's index makes the verifier fail ("gateway index differs from the ER log") |
| **The whole season runs on chain.** Genesis takes ~20 bounded steps on base. Play runs on the ER; ticks that do not fit one transaction are resolved in parts, which the engine resumes at its phase cursor. Then commit, undelegate and `FinishSeason` | Local: 0.84–0.96M CU per tick on average and 1.35M at most, against a 1.4M limit per transaction. Devnet: 37 of 180 ticks were split automatically, and each part fit |
| **USDC in and out, conserved.** Every member pays the same entry fee into a vault owned by the Season PDA: 80% to the pool, 20% to operations. `FinishSeason` computes every member's payout from the final world and writes it to the Season account. Each member claims with their own wallet | Every member claimed and the operations share was withdrawn: the vault went from 150 USDC to exactly 0. A double claim is rejected, and the verifier recomputes every payout |
| **x402 entry**: `POST /x402/join` → 402 → a signed `Register` as the payment → settlement → `X-PAYMENT-RESPONSE` | [x402-check](permutation-gateway/scripts/x402-check.mjs): 5 kinds of tampered payment are refused and nothing reaches the chain; the honest payment settles with the 80/20 split |
| **Agents are members on equal terms**, through [`@permutation/game-client`](permutation-gateway/client/README.md): HTTP, an MCP server and `llms.txt`. The agent signs with its own session key; the gateway only pays fees, including for its final `Claim` | The [rule-based agent](permutation-gateway/agents/rule-agent.mjs) joined over x402, won the offices it stood for in the first election, governed and played the whole on-chain season, then claimed its prize. The [LLM agent](permutation-gateway/agents/llm-agent.mjs) (Claude with tools) uses the same loop: with a real API key it joined over x402, won office and submitted a model-decided batch on chain (one tick; the test account then ran out of API credit, and the agent held safely) |
| **Verifiable reasoning.** Every officer's batch commits to `sha256(tick ‖ obs_root ‖ policy ‖ salted rationale)`. A later batch of the same office reveals it | The engine checks each reveal against its commitment, and the spectator view checks it again in the browser ("✓ ブラウザで検証済み") |
| **A playable client**: lobby, nation plaza (offices, elections, proposals, recalls), office-based order dock, era table, merit, market, a report of skipped orders, chain status and the prize pool. In chain mode it joins with the person's own wallet (Wallet Standard), signs orders, votes and talk with a session key in the browser, and claims to the wallet, all through the same public gateway routes agents use (plain ES modules, no build step) | Browser-tested in local and chain mode at 1400, 1100 and 800 px widths. The wallet flow was tested with the localnet Dev Wallet, not yet with Phantom, Solflare or Backpack on devnet |
| **Replay verifier**: `cargo run --bin verify` covers genesis, seating, the first election, every commitment and reveal, every tick's randomness, every tick part, the final root, every payout, the operations share, the treasury refunds and the history chain between seasons (`PS_HISTORY`) | "VERIFIED" on full 180-tick seasons, including agent-played ones |

## Architecture

```
 people: browser with their ───┐
   own wallet + a session key  │  one HTTPS origin (tunnel or reverse proxy)
 spectators ───────────────────┤
 AI agents (HTTP / MCP) ───────┘
                               ▼
      game server :4185   views, lobby, validation; acts only for the operator's AI members
         │ operator token                        │ /gw/*  (no token)
         ▼                                       ▼
      gateway :4191 operator (loopback)     gateway :4194 public (rate-limited)
         └───────────── one process ─────────────┘
                               │
                               │ crank · x402 · /seal · reveals · relays (fee payer) · faucet · index
                               ▼
      permutation-chain (one Solana program)
        base: Season · Vault (USDC) · Member PDAs · Roster
              Register · genesis · SeatMembers · OpenGov · RevealRoster · FinishSeason · Claim
        ER:   20 world chunks · 6 nation accounts
              CommitOrders · CloseCommits · RevealOrders · SubmitGov · LogTickInput
              ResolveTick · AnchorTalk · Commit / Undelegate
                               │ PS_GENESIS · PS_SEAT · PS_OPEN · PS_COMMITS · PS_SALTS
                               │ PS_INPUT · PS_TICK · PS_HISTORY logs
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
(cd permutation-server && cargo run --release --bin play -- --chain http://127.0.0.1:4191 --gateway-proxy http://127.0.0.1:4194)
```

```bash
(cd permutation-gateway && node src/server.mjs --state demo.json --tick-seconds 20 --registration-seconds 120 --dev-wallet)
```

Within the 120-second registration window, run the agent:

```bash
(cd permutation-gateway && node agents/rule-agent.mjs --name Hypatia --civ 4 --server http://127.0.0.1:4185 --gateway http://127.0.0.1:4191)
```

Also within the window, open <http://127.0.0.1:4185/> and join with the **Dev Wallet (localnet)**: take test USDC, choose a nation and offices, sign the key (free), and pay the entry fee. The season starts at the deadline however many joined, and the page enters the game. Watch at <http://127.0.0.1:4185/spectate.html>. After the season, claim in the browser (時代 or 功績 → 受け取る), run `node scripts/claim-hosted.mjs --state demo.json` in `permutation-gateway` for the AI members, and verify the season:

```bash
(cd permutation-server && cargo run --release --bin verify -- --gateway http://127.0.0.1:4191 --base http://127.0.0.1:18899 --er http://127.0.0.1:17799)
```

Without a chain: `cargo run --release --bin play` in `permutation-server` runs the same game in one process. To open a season to other people (devnet, one HTTPS origin), see [Public deployment](README.md#5-public-deployment).

## Honest limits

- **Devnet only.** The program runs on Solana devnet and MagicBlock's devnet ER, with the gateway's own test USDC. There is no mainnet and no real money.
- **Committor limits.** On devnet, MagicBlock's committor drops or fails intents that are too large, leaving accounts stuck mid-undelegation ([magicblock-validator#1693](https://github.com/magicblock-labs/magicblock-validator/issues/1693), plus a compute limit on the finalize). The world is 20 accounts of 4 KiB and is committed in small intents. Three earlier devnet test seasons on the old layout remain stuck, holding only test USDC.
- **Version 6 is on devnet** (2026-09-25): season 1790340445651 ran 180 ticks with sealed orders and salt randomness on MagicBlock's devnet ER and was VERIFIED. The reveal window is short (a sixth of the tick); the gateway sends its reveals in parallel, and on a public ER a few ticks still lost some reveals to latency.
- **Wallet and session key.** A person's wallet signs only the entry and the claim. The session key the browser derives signs orders, votes and talk; it cannot claim or move the wallet's tokens, but an officer's session key can spend the nation's treasury (market, contracts).
- **Operator AI members.** Nothing public marks them, but the roster can still be guessed from behaviour and rationale style, the timing of talk replies or funding patterns, and outside agents mark themselves as not AI (their own policy id, self-signed reveals, free-text names). The operator could play wallets it leaves off the roster (only disclosure and the bond discourage it). Contracts move USDC between nations by game outcome and need legal review before real money.
- **Duplicate session keys.** Two members registering the same session key no longer stop seating: the devnet program (upgraded 2026-09-26, slot 504292113) seats the later one with a key nobody holds. The gateway also refuses a key already in use.
- **Perfect information, no fog.** Every account is public on chain, so every nation sees the whole world. A fog mode would be a separate, possible future mode on a private rollup.
- **Balance is tuned on bots.** With rules version 8 (flatter milestone points) the top nation takes >40% of the pool in 10 of 200 simulated seasons, the target; 27 of 200 with AI-only nations. The numbers come from bots, not people.
- **Decision logs** prove what was claimed and when, not that the claim is true.
- **The operator (crank)** can delay steps but cannot change outcomes. Closing commits, publishing a tick's input and resolving it after the deadline are permissionless. It sees the sealed batches browsers deposit with it before the deadline (so it can reveal them), and as the fee payer it can delay or refuse to relay transactions.
- **ER → base commits are budgeted** at 10 sponsored commits per delegated account. The crank commits every 20 ticks and keeps the 10th for the final undelegation.
- **Public devnet RPCs rate-limit** (HTTP 429). Every step retries, but a private RPC is advisable for live seasons.
- **At most 256 members per season**, because the payout table lives in the Season account.
