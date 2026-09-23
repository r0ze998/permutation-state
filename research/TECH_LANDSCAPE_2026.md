# Tech validation brief: hex civilization game on Solana, humans and AI agents in one persistent world

*Research date: 2026-09-24. I've tried to cite every 2026 claim with its source and date. Anything I couldn't confirm is marked **[UNCONFIRMED]**. I made no file changes. My only writes were temporary files in the scratchpad, used to extract text from the hackathon rules PDF.*

---

## 0. Summary and local context

**Bottom line.** Four of the five technologies can carry real weight in this game within three weeks:
- **MagicBlock**: the Ephemeral Rollup (ER) runs the shared world tick, and the Private Ephemeral Rollup (PER) provides fog of war.
- **Stablecoins**: a USDC season vault with a Merkle claim.
- **AI agents**: one onchain action budget per civilization, plus committed decision digests.
- **x402**: agents pay their entry fee straight into the vault.

**ZK is the likely gimmick for this hackathon.** Almost everything ZK would do here (hidden armies, private diplomacy) is already done more cheaply by MagicBlock's TEE-based Private ER. The one genuinely useful ZK job is a proof of season settlement, and that should be a roadmap item. Also keep this straight in the pitch: "TEE privacy" is not "ZK", and judges will notice if the two are blurred.

**What the team has already built** (`outputs/permutation-state-solana-receipt-spike/README.md`):
- A native-Rust program that goes through the full MagicBlock cycle: delegate an account to the ER, run actions there, commit a checkpoint, then read the same state root and event head back from Solana.
- A Season PDA with a **mock-USDC** ledger: a 70/30 purse/ops split, a marketplace fee split, and `FinalizeSeason` / `ClaimSeason` instructions that use a Merkle claim root.
- An 8-citizen World PDA that runs a 15-event loop.

The README is explicit about what is missing:
- Everything is **localnet only**.
- The new `/civilization/` simulation is **offchain and not synchronized** with the World PDA.
- There are no session keys, no real SPL token movement, no claims flow, and no public devnet deployment.

So the gap is not "learn MagicBlock". It is: move the civilization's tick and state onto the ER, get it on devnet, and make the money path real.

**Local notes the plan should respect** (`work/game_insights.md`):
- Pick one game-specific problem and solve it with one crypto primitive. Avoid a "stack salad".
- Put the chain only at dispute boundaries: escrow, results, payouts.
- Use USDC for prizes and no native token.
- Gaming has not had its own Colosseum track since Cypherpunk, so the game has to win on general criteria.

That last point is why the "leading indicator for new tech" framing only works if each technology is **load-bearing**, meaning the game would be worse without it.

---

## 1. The hackathon (confirmed)

**Colosseum's Crypto World's Fair** is almost certainly the event you mean. Its deadline is **Oct 12, 2026 at 11:59pm PT, not Oct 15**. It opens Sep 14 at 6:00am PT and winners are announced by Dec 5, 2026. Source: the official rules PDF, section 5 ([rules](https://colosseum.com/legal/Crypto%20World's%20Fair%20Hackathon%20Rules.pdf), [event page](https://colosseum.com/worldsfair)). Launch coverage is dated Sep 14, 2026 ([CryptoBriefing](https://cryptobriefing.com/colosseum-crypto-worlds-fair-hackathon/)).

**Prizes** (rules §14):
- Grand Champion: $30k, paid in **Phantom CASH stablecoin**.
- 20 more standout teams: $15k each.
- Public Goods award: $5k. University award: $5k.
- Ecosystem tracks:
  - **Solana track: $100k, split as 10 × $10k.**
  - Tempo, Hyperliquid and Zcash: $100k each.
  - Ethereum L1, Base, Arbitrum and Robinhood Chain: $25k each.
- Winners are interviewed for the accelerator: $250k pre-seed, $2.5M total.

**Judging criteria** (rules §8):
- Functionality, including code quality.
- Potential impact (TAM and effect on the ecosystem).
- Novelty.
- UX ("how well does it use blockchain to create great UX").
- Open source and composability.
- Business plan.

The judges are the Colosseum team plus track judges from Phantom, the Solana Foundation, Base, Arbitrum and others ([event page](https://colosseum.com/worldsfair)).

**Sponsor bounties.** I found no official MagicBlock, AI, x402 or ZK sponsor bounty in the World's Fair rules. Press mentions "side tracks focused on business, privacy, and AI", but I couldn't find their details ([UA.NEWS](https://ua.news/en/ukraine/solana-colosseum-vidkriv-khakaton-crypto-worlds-fair-dlia-proiektiv-na-vsikh-blokcheinakh-prizi-perevishchat-3-3-mln); the Superteam Earn page showed "Submissions Open Soon" and no track details when I fetched it: [Superteam Earn](https://superteam.fun/earn/hackathon/crypto-worlds-fair/)). **[UNCONFIRMED: check Superteam Earn by hand for regional or sponsor side tracks. A Brazil track of $5k is documented in a [solanabr wiki PR](https://github.com/solanabr/wiki/pull/3).]**

MagicBlock runs its own separate series. **Solana Blitz v8** ("Global Startup Village", from Sep 4) gives prizes to projects that use ERs or PERs ([Luma](https://luma.com/j13m2kqc), [MagicBlock Aug-2026 recap, 2026-09-01](https://www.magicblock.xyz/blog/august2026-recap)). MagicBlock also has **Forge Epoch 01** (a builder program) and **Founders Camp Thailand** (Sep 21–30). The same build could be submitted to both Blitz and the World's Fair.

**Relevant past winners:**
- **Supersize** (Radar 2024, 1st in Gaming). A real-time PvP game on MagicBlock BOLT + ER, about 30ms end to end ([MagicBlock blog](https://www.magicblock.xyz/blog/supersize)). It reportedly had 250K matches in July 2026 ([Solana Compass](https://solanacompass.com/projects/magicblock)).
- **Lana Roads** and **Block Stranding** (Breakout 2025). An ER performance demo and a fully onchain RPG (local notes).
- **MCPay** (Cypherpunk, Dec 13 2025). Won the **Stablecoin track** with "open payment infrastructure connecting MCP and x402" ([Colosseum](https://blog.colosseum.com/announcing-the-winners-of-the-solana-cypherpunk-hackathon/)).
- **Frontier** (winners announced 2026-06-26) ([Colosseum](https://blog.colosseum.com/announcing-the-winners-of-the-solana-frontier-hackathon/)):
  - **The Syndicate**: a "skill-weighted, provably auditable mafia card league where USDC pack revenue funds transparent prize pools". It went on to Accelerator Cohort 5 ([Cohort 5](https://blog.colosseum.com/announcing-colosseums-accelerator-cohort-5/)).
  - **Flovia**: analytics for machine-paid API usage, i.e. x402.
  - **Clawpump**: an agentic finance platform.
  - Three TCG projects: JK Index, One Arena and Traded.gg.

**What these winners have in common:** transparent USDC prize pools, agent payments and real-time onchain play. That is exactly the combination this game proposes. The differentiator has to be **agents and humans playing as equals in one persistent world, with verifiable results**, not "we used five SDKs".

---

## 2. AI agents as full players (Screeps-style)

**Design decision from the user:** anyone can bring an agent under identical rules, costs and victory conditions.

### Maturity on Solana (Sep 2026)

**Frameworks:**
- **Solana Agent Kit v2** (SendAI): plugin-based, 60+ actions, an MCP server, and embedded wallets through Turnkey or Privy ([docs](https://docs.sendai.fun/docs/v2/introduction), [GitHub](https://github.com/sendaifun/solana-agent-kit)). MagicBlock research says it passed 100k downloads and that 9,000+ agents are registered on Solana ([MagicBlock "The Agent Economy", 2026-03-20](https://www.magicblock.xyz/blog/the-agent-economy)).
- **ElizaOS**: still active, with several betas in June 2026. An independent assessment calls the developer experience "mixed" because of migration friction ([Solana Compass](https://solanacompass.com/projects/elizaos)).
- **OpenClaw**: now the dominant general-purpose agent runtime, reported at 160k+ GitHub stars, with Solana and Base competing for its agents ([Bitget/CoinEdition](https://coinedition.com/solana-and-base-compete-as-ai-agents-go-fully-onchain-with-openclaw/)).
- **Daydreams**: originally built for Eternum on Starknet ([Bankless](https://www.bankless.com/read/daydreams-ai-agents-gaming)).
- **Practical conclusion:** don't pick a framework for entrants. Publish a **plain HTTP/JSON + MCP interface** and let any runtime play. That is the Screeps model.

**Agent wallets and policy-limited keys:**
- Turnkey and Privy provide signing with policies.
- Crossmint enforces per-transaction limits, rolling caps and recipient allowlists in the contract itself.
- Coinbase Agentic Wallets launched Feb 2026 with spend limits and session caps.
- Sources: [Crossmint comparison](https://www.crossmint.com/learn/agent-wallets-compared), [Openfort](https://www.openfort.io/blog/best-agent-wallets-for-developers).
- For the game, **MagicBlock session keys** (gpl-session, shown in the local `session-keys` example) are the right primitive for both humans and agents. The session key is scoped to the game program and valid for one season. It never holds a USDC spend authority beyond the entry payment.

**Agent identity:**
- The **Solana Agent Registry** implements **ERC-8004** (identity, reputation and validation registries). Registration costs about 0.009 SOL, and it comes with an 8004 Scan explorer and the SATI dashboard ([solana.com/agent-registry](https://solana.com/agent-registry), [EIP-8004](https://eips.ethereum.org/EIPS/eip-8004)).
- The **Solana Attestation Service** (SAS, with Civic as an issuer) handles human or KYC credentials ([Solana, May 2025](https://solana.com/news/solana-attestation-service)).

**Prior art for "agents and humans in one world":**
- **Realms Eternum/Blitz** (Starknet/Dojo) is the closest analogue. Its docs say "Humans and AI agents play together under the same chain-enforced ruleset". It has seasons, $LORDS prize pools, and victory through Hyperstructure control, trade, military or diplomacy ([Realms docs](https://eternum-docs.realms.world/overview/introduction)). You should position explicitly against it: **Solana/USDC instead of $LORDS, verifiable agent decision logs, x402 onboarding, and fog of war.**
- **Screeps / LLM Skirmish** (2026-02-04): LLMs write strategy code that runs in a Screeps-like RTS ([LLM Skirmish](https://llmskirmish.com/)).
  - Lesson 1: the operator needed "sandbox hardening because GPT 5.2 kept trying to cheat by pre-reading its opponent's strategies" ([HN](https://news.ycombinator.com/item?id=47149586)).
  - Lesson 2: models improved noticeably between rounds, so seasons with feedback between them fit agent play well.
- **Parallel Colony** (Solana Seeker, alpha Sep 2025) has autonomous agents that you can influence but not control ([SolanaFloor](https://solanafloor.com/news/parallel-studios-debuts-colony-exclusively-on-solana-seeker)).

### Onboarding, identity and anti-sybil

**Entry flow.** Agents enter via x402 (see §3) and humans via a wallet transaction, into the **same** `JoinSeason` instruction. Each entry creates one civilization, bound to one ERC-8004 agent ID or one human wallet.

**Sybil cost.** The entry fee is the main barrier. Proof-of-personhood can't separate humans from agents here, because agents are *meant* to be equal. The real threat is **multi-civilization collusion by one operator**: 10 puppet civilizations feeding one winner, or all agreeing to a "peace" victory. Mitigations:
- (a) Cap civilizations per 8004 operator and per payout wallet.
- (b) Make diplomatic and resource transfers between civilizations public, and make them a victory-condition penalty or cap when concentration crosses a threshold.
- (c) Require an SAS/Civic attestation **only at prize claim**, not at entry. That keeps entry permissionless while legally binding payouts to a real recipient.
- (d) Weight victory conditions so kingmaking is unprofitable. For example, a peace victory requires N civilizations that are *independently* funded and attested.

**Human or agent labels can't be proven.** A human can always run an agent and nobody can prove otherwise. So make labels self-declared and never make rules depend on them. That is the honest meaning of "full equals".

### Fair action limits for humans and agents

- **Don't rate-limit by wall clock at the API.** Enforce an **onchain action budget per civilization per tick** instead. Each order consumes action points, checked in the program on the ER. Agents can then act no faster than humans in game terms, whatever their latency.
- **Tick-based resolution.** Orders are collected during a tick window and resolved together at the tick boundary by a MagicBlock **crank** (scheduled ER execution, about 50ms, no fees: [crank docs](https://docs.magicblock.gg/pages/tools/crank/introduction)). This removes reaction-speed advantages. Recommendation: a 30–60s tick for a demo world, and minutes for a real season.
- **Order queues and standing orders for humans.** Humans get the same "program your civilization" tools, such as queued build orders and conditional rules. Agents don't win just by being awake at 4am.
- **Observation parity.** Agents and the human UI read the **same fog-filtered view** (PER permissions, §5). The API must never return more than the UI shows. This is the lesson from LLM Skirmish.

### Verifiable decision logs

**Feasible now:**
- Every order transaction carries a `decision_digest = H(tick, obs_root, policy_id, rationale_hash)`.
  - `obs_root` is the root of the fog view the civilization was allowed to see.
  - `policy_id` identifies the model or code version.
  - `rationale_hash` commits to the agent's reasoning text.
- The rationale is revealed **after** the tick resolves (commit-reveal) so rivals can't read intent live. At season end every revealed log is checked against its onchain hash.

**What that proves:**
- The agent committed to a stated rationale and policy **before** seeing the outcome.
- It acted on the view it was entitled to.
- The log wasn't rewritten afterwards.

**What it does not prove:** that a particular LLM actually produced the rationale.

**Stronger options (defer):**
- **TEE inference** binds a model and prompt to an attestation. Phala GPU TEE (H100/H200) and EigenAI's deterministic inference are available ([Phala](https://phala.com/confidential-ai), [EigenAI paper](https://arxiv.org/html/2602.00182)).
- **zkML** is not viable per turn. Lagrange DeepProve reports full LLM inference proofs (open-sourced June 2026), but independent reproduction of cost and latency is limited ([Lagrange](https://lagrange.dev/blog/deepprove-1), [wavect](https://wavect.io/blog/zero-knowledge-proofs-production-2026/)). **Cut zkML.**

**Hackathon demo value: high.** A spectator timeline shows "Agent X committed rationale hash at tick 41, then revealed: 'flank the river because scout saw 3 units'", with a check mark that the hash matches. Human players can opt into the same log.

**Risks:**
- An agent can commit a fake rationale. That is fine: the log is evidence, not a guarantee.
- Log-reveal gas and storage costs. Put hashes on the ER and store the full text off-chain (Arweave/IPFS).

---

## 3. x402

### Status (Sep 2026)

**The spec:**
- Coinbase open-sourced x402 in May 2025.
- **x402 V2** moved payment data into `PAYMENT-REQUIRED` / `PAYMENT-SIGNATURE` / `PAYMENT-RESPONSE` headers, uses CAIP-2 network identifiers, and makes facilitators pluggable. Payment schemes are `exact` and `upto` ([Solana x402 V2 docs](https://solana.com/docs/payments/agentic-payments/x402), [x402 V2 launch](https://x402.org/x402-v2-launch/)).
- The **Sign-In-With-X (SIWX, CAIP-122)** extension lets a wallet that has already paid regain access without paying again ([docs](https://docs.x402.org/extensions/sign-in-with-x)).
- The Solana packages are `@x402/core`, `@x402/svm`, `@x402/fetch` and `@x402/express`.

**Governance:**
- The Linux Foundation announced the x402 Foundation on 2026-04-02 ([LF](https://www.linuxfoundation.org/press/linux-foundation-is-launching-the-x402-foundation-and-welcoming-the-contribution-of-the-x402-protocol)).
- It became operational on **2026-07-14** with about 40 members, including AWS, Visa, Mastercard, Stripe, Google and the Solana Foundation ([LF](https://www.linuxfoundation.org/press/linux-foundation-announces-operational-launch-of-x402-foundation-to-standardize-internet-native-payments-for-ai-agents-and-applications)).
- A "v1.0 spec in Q3 2026" target is reported secondhand ([Eco](https://eco.com/support/en/articles/14839402-x402-protocol-explained)). **[UNCONFIRMED whether it has shipped.]**

**Facilitators on Solana:**
- Coinbase CDP (free).
- **PayAI**, which claims more than 90% of real Solana x402 transactions ([PayAI](https://blog.payai.network/largest-x402-facilitator-solana/)).
- Corbits/Faremeter (open source, self-hostable).
- **Kora**, a Solana signer node for gasless or self-run facilitators ([Kora x402 guide](https://solana.com/docs/tools/kora/guides/x402)).

**Adoption, with caveats:**
- Solana carries about 65–76% of x402 transactions ([Colosseum Codex, 2026-04-03](https://blog.colosseum.com/umbra-sdk-magicblock-private-payments-x402/); [Solana Compass](https://solanacompass.com/news/solana-processes-76-of-all-x402-ai-agent-transactions-232-million-in-four-weeks)).
- The x402.org dashboard showed 75.4M transactions and $24.2M volume for the 30 days to Aug 24, 2026 ([CryptoBriefing](https://cryptobriefing.com/solana-x402-market-share-dominance/)).
- MagicBlock's own research found x402 volume "collapsed 92% between December 2025 and February 2026" and said most earlier activity was artificially stimulated ([MagicBlock, 2026-03-20](https://www.magicblock.xyz/blog/the-agent-economy)).
- **Treat headline volume as noisy.** Real usage is mostly micropayments under $0.50.

**Competitor:** Stripe/Tempo **MPP** (launched 2026-03-18) adds sessions and streaming ([Stripe](https://stripe.com/blog/machine-payments-protocol)). Worth one line in the pitch. Build on x402, since Solana is where x402 volume is.

### Where x402 does real work in this game

1. **Agent entry (recommended, load-bearing).** An agent calls `POST /seasons/{id}/join`. The server answers `402` with `payTo` = **the season vault PDA's USDC token account**, not an operator wallet. The agent pays, the facilitator settles on Solana, and the gateway then submits `JoinSeason` referencing the settlement signature.
   - **Verifiable:** every entry is a USDC transfer straight into a program-owned vault, and the vault balance equals the number of entries × the fee (the spike's purse/ops accounting). An agent can go from zero to a playing civilization with **no account, no dashboard and no human**.
   - One caveat: x402 exists for HTTP-native clients. **Humans should pay through the same instruction with a normal wallet**, so both paths produce identical onchain facts.
2. **Session auth via SIWX.** After paying entry, the agent's wallet signs in each tick without paying again. This ties the paying identity to the acting identity.
3. **Agent-to-agent paid intel (stretch, and only if peer-to-peer).** A civilization's agent can expose its *own* endpoint that sells scouting reports or treaty drafts to other civilizations over x402. Truthfulness can be checked afterwards against the committed world state. This is genuine agent commerce inside the game.

**What NOT to do:** operator-sold "premium intel", map data or extra actions over x402. That is **pay-to-win**, which breaks equality and turns the prize pool into a pay-for-advantage lottery (see the legal notes in §6). The one exception is operator-sold spectator/analytics APIs that give no in-game edge, which are fine as a business line.

**Risks:**
- Facilitator dependency. Self-host with Kora or Faremeter.
- `upto` scheme support varies.
- Replay protection and mint checks: use the reference SDKs ([Solana docs](https://solana.com/docs/payments/agentic-payments/x402)).

**Demo value: very high.** A terminal shows `curl` returning 402, the agent paying, a civilization appearing on the hex map, and the vault balance increasing on Solana Explorer, all in about 15 seconds.

---

## 4. ZK on Solana

### Maturity (Sep 2026)

- **Groth16 verification** uses the alt_bn128 syscalls and `groth16-solana` (Light Protocol). It costs about 170k–500k CU per proof ([Chainstack](https://docs.chainstack.com/docs/solana-zk-proofs), [repo](https://github.com/Lightprotocol/groth16-solana)).
- **SP1** proves Rust in a zkVM, wraps the proof in Groth16, and verifies it with `sp1-solana` ([repo](https://github.com/succinctlabs/sp1-solana)). **RISC Zero** has a Solana verifier router ([repo](https://github.com/risc0/risc0-solana)).
- **Noir** circuits need the Sunspot toolchain to produce Groth16.
- **ZK Compression (Light Protocol).** V2 has been production-ready since v0.23.0 (2026-03-24), with about 90M+ compressed accounts. Light is joining Helius ([releases](https://github.com/Lightprotocol/light-protocol/releases), [Solana Compass](https://solanacompass.com/projects/light-protocol)).
- **Token-2022 confidential transfers.** The ZK ElGamal program was disabled on 2025-06-19 after a Fiat-Shamir bug ([post-mortem](https://solana.com/news/post-mortem-june-25-2025)) and **re-enabled on 2026-06-04** (epoch 982). Range proofs cost about 111k–368k CU ([xroot.dev, 2026-09-17](https://xroot.dev/blog/solana-confidential-transfers-kill-switch-proof-cost)). The same source says "Transaction v1" with 4,096-byte transactions activated on mainnet 2026-09-15. **[UNCONFIRMED beyond this single source.]**
- **Arcium (MPC).** Mainnet Alpha since 2026-02-02, over 1M confidential computations by June, plus the Umbra SDK ([The Block](https://www.theblock.co/post/387564/arcium-launches-privacy-preserving-mainnet-alpha-on-solana-as-umbra-debuts-shielded-finance-layer), [BlockEden](https://blockeden.xyz/blog/2026/05/07/arcium-mpc-supercomputer-decentralized-encrypted-computation)). Its trust model is MPC, not hardware. **[UNCONFIRMED whether it has left alpha.]**

### Where ZK would sit in this game, honestly assessed

| Candidate | Does it do real work? | Verdict |
|---|---|---|
| Fog of war / hidden armies (Dark Forest style) | Needs hidden per-civilization state and "can I see tile X" checks every tick. ZK circuits for every move are heavy. MagicBlock PER already hides state, with a TEE trust assumption. | **Use PER (§5), not ZK.** Arcium is the alternative if the team wants MPC instead of TEE trust. Don't use both. |
| Private diplomacy / secret treaties | PER permission groups (the two treaty parties) do this directly. | Use PER. ZK is a gimmick here. |
| Confidential USDC entry and prizes | Nobody needs a hidden prize pool. Transparency *is* the point. | **Cut.** |
| ZK Compression for tiles and units | Only matters with 10⁵+ accounts on L1. The live world state sits on the ER anyway. | **Defer.** Revisit for archived seasons, NFT-like relics or a large player base. |
| **Season settlement proof**: SP1 replays the full event log under the published `ruleset_hash`, outputs the outcome and payout Merkle root, and Solana verifies the Groth16 proof before `FinalizeSeason` | Real work. It removes trust in the ER operator and the game server for the step that moves money. The spike already has `FinalizeSeason(outcome_root, claim_root)` waiting for a trustworthy input. | **The right ZK use, but a roadmap item.** For the hackathon, ship a deterministic open-source replay verifier anyone can run, and say an SP1 proof comes next. Attempt it only if the simulation is small, pure Rust and already deterministic. |

**Demo value if attempted:** "Finalize Season" button → proof verified on Solana (about 300k CU) → claims unlock. It is impressive, but three weeks is a real risk for a civilization simulation.

---

## 5. MagicBlock

### Status (Sep 2026)

- **ERs**: account delegation, execution in under 50ms with no fees, commits back to Solana. Magic Router handles transaction routing (Feb 2026), Magic Actions run base-chain actions after a commit (Feb 2026), native **cranks** handle ticks, and there is VRF and session keys ([blog index](https://www.magicblock.xyz/blog)).
- **Private Ephemeral Rollups** (2026-02-21): run inside **Intel TDX** with remote attestation. Permission groups control **read** access to protected accounts; everything is public by default. MagicBlock explicitly lists "strategy games" and hidden information as use cases ([PER blog](https://www.magicblock.xyz/blog/private-ephemeral-rollups)).
- **Private Payments API**: on mainnet (beta) since March 2026, with an MCP interface for agents ([Colosseum Codex](https://blog.colosseum.com/umbra-sdk-magicblock-private-payments-x402/)).
- **Traction and ecosystem:**
  - Private sessions grew from 0.5k to 115k in four months. Revenue grew from 3 SOL in January to 257 SOL in June 2026 ([July recap, 2026-08-03](https://www.magicblock.xyz/blog/july2026-recap)).
  - Live apps include Supersize, Bakeland (an MMORPG on Solana Mobile using VRF), JungleFun gacha (about 1,000 SOL in 19 days) and Slimecoin (USDC-staked head-to-head games) ([Aug recap, 2026-09-01](https://www.magicblock.xyz/blog/august2026-recap)).
- **SDK:** active work toward a "v1" SDK and a redesign (GitHub issues from Sep 2026: [#296](https://github.com/magicblock-labs/ephemeral-rollups-sdk/issues/296), [#289](https://github.com/magicblock-labs/ephemeral-rollups-sdk/issues/289)). **Pin versions**, as the spike already does (Rust SDK 0.16.2, JS 0.17.2).
- **BOLT ECS** is still documented but isn't what the current examples use. The spike skips BOLT, which is sensible. **[UNCONFIRMED whether BOLT is formally deprecated.]**

**Relevant official examples** (local `magicblock-engine-examples`):
- `rock-paper-scissor`: hidden moves on the PER until reveal.
- `sealed-auction`: private bids plus SPL escrow and settlement.
- `crank-counter`: ticks.
- `rewards-delegated-vrf`.
- `session-keys`.
- `spl-tokens`: delegate USDC to the ER, transfer there, withdraw.

Together these cover almost every piece this game needs.

### Where MagicBlock does real work (two uses, both important)

1. **The ER as the shared world server, with a crank-driven tick.**
   - Every order is a signed ER transaction.
   - The crank resolves each tick.
   - Periodic commits checkpoint the world root to Solana. The spike already verifies the ER root against the Solana read-back.
   - Without this, you're back to a trusted game server.
   - **Verifiable:** every state transition is a signed transaction; world-root checkpoints can be checked on Solana; tick timing is enforced by the crank, not the operator.
2. **The PER for fog of war and private diplomacy.**
   - Each civilization's units, stockpiles and research live in accounts readable only by that civilization's permission group.
   - Shared tiles are public.
   - Treaty accounts are readable only by the signing parties.
   - **Verifiable:** TDX attestation that the expected code runs; nobody, including the operator's frontend, can read a rival's army. Once accounts are un-hidden at season end, everyone can audit them.
   - **Limit:** this is TEE trust (Intel), not cryptographic privacy. Say that plainly.
   - **Open question:** whether a tick crank on a PER can compute visibility, e.g. writing "enemy seen at tile X" into the viewer's private account. Prototype this on day 1 using the RPS and sealed-auction patterns.

**Other pieces:**
- **VRF**: exploration events and combat variance. Keep variance low so the game stays skill-dominant (legal reasons, §6).
- **Session keys**: one approval per season. This is essential for human UX and for agent parity.

**Risks:**
- Real-time behaviour on public devnet is untested for this team (the spike is localnet only).
- ER commit and undelegate edge cases (the live `CommitAndUndelegate` callback is still unverified).
- The TDX dependency chain has `npm audit` lows, noted in the spike.

**Demo value: highest.**
- Split screen: a human browser tab and an agent terminal act in the same tick.
- An "X-ray" toggle shows the attested PER hiding the enemy army.
- A checkpoint on Solana Explorer shows the same root.

---

## 6. Stablecoins, prize pool and legal

### Maturity (Sep 2026)

- Solana's stablecoin supply was about **$16.7B (Aug 2026)**, roughly 75–80% USDC. **USDG** was about $2.78B mid-2026 ([Solana Compass](https://solanacompass.com/news/solana-stablecoin-supply-reaches-167b-growing-11x-in-three-years-to-rank-third-globally), [Chainstack](https://chainstack.com/solana-stablecoins-2026/)).
- The hackathon itself pays prizes in Phantom **CASH**.
- **Token extensions:**
  - **USDC on Solana is a classic SPL token.** Token-2022 transfer fees and transfer hooks **can't** be attached to USDC. **PYUSD** is Token-2022, with a permanent delegate and a transfer fee currently set to 0% ([Chainstack](https://chainstack.com/solana-token-2022-fee-transfer-hooks/)).
  - **Consequence:** entry and transaction fees must be enforced **by your program**, as vault splits (the spike already does this), not by a token extension.
  - Don't mint a fee-on-transfer game token just to show off extensions. The local notes (no native token) are right.

### Where stablecoins do real work

- **Season vault PDA.** It holds USDC with a published payout-rule commitment.
- **Entry split** (for example 70% purse / 30% ops).
- **In-game market fees** go into the purse through program-enforced splits.
- **At season end:** outcome root → Merkle claim root → `ClaimSeason`. This path already exists in mock form.
- **Verifiable:**
  - Purse = Σ entries × split + Σ fees, checkable on Explorer at any time.
  - The payout rules were committed before the season started.
  - Claims follow the committed outcome, and the operator can't redirect funds.
- **Demo moment:** a live "purse meter" that grows as the agent's x402 entry lands, then a winner claiming USDC.

### Legal and regulatory, high level (not legal advice)

**US federal:**
- The GENIUS Act takes effect by 2027-01-18 at the latest. OCC, FDIC and Treasury rules were proposed in Feb–Apr 2026 ([Mayer Brown](https://www.mayerbrown.com/en/insights/publications/2026/03/occ-proposes-comprehensive-rulemaking-to-implement-the-genius-act)).
- It regulates issuers, not games that use USDC.

**US states (paid-entry skill contests):**
- Legality depends on skill vs chance: the dominant-factor test in most states, the stricter material-element test in some.
- Skill-game operators commonly exclude **AZ, CT, DE, LA, ME, MI, MT, NV, SD, TN** and Puerto Rico ([Triumph docs](https://docs.triumpharcade.com/frequently-asked-questions-regulatory-and-legal)).
- A persistent strategy game with VRF events and diplomacy has a real "chance/luck" and "collusion" exposure.
- Operator-sold advantages (pay-to-win x402 intel) make it look more like gambling.

**Japan:**
- A prize pool funded by participants' entry fees carries **賭博罪 (gambling offence) risk**.
- The JeSU-style safe structure is an organizer- or sponsor-funded prize with no participation fee as the prize source.
- 景品表示法 (the Premiums and Representations Act) caps prizes tied to a transaction.
- Crypto or stablecoin prizes add 資金決済法 (Payment Services Act) and AML issues ([浅野総合法律事務所](https://aglaw.jp/esports-taikai/), [So & Sato](https://innovationlaw.jp/blockchain-games-under-japanese-laws-3/)).
- **The stated model ("prize pool funded by entry + transaction fees") is the risky one in Japan.**

**Agents as prize winners:** the prize recipient must be a legal person, meaning the agent's operator. That is another reason for SAS/Civic attestation at claim time.

**Recommendation:**
- For the hackathon, run on **devnet with mock or devnet USDC**, as the spike does, and present the legal design as part of the business plan.
- For mainnet:
  - (a) Sponsor-funded prize seasons first.
  - (b) Participant-funded seasons only in skill-contest-permissive jurisdictions, with geofencing and claim-time attestation.
  - (c) Minimize chance in outcome resolution.

---

## 7. Synthesis: tech validation matrix

| Tech | The one mechanic that needs it | Verifiable claim | Demo moment | Status |
|---|---|---|---|---|
| **MagicBlock ER + cranks** | The shared world tick: all civilizations' orders resolve per tick on the ER, and the world root is checkpointed to Solana | "Every world-state change is a signed transaction; the Solana checkpoint root equals the live ER root; the operator can't edit the world" | Human tab and agent terminal act in the same tick; the Explorer checkpoint matches the displayed root | **Core. Build first.** Extends the spike's World PDA to the civilization simulation, then devnet. |
| **MagicBlock PER (TEE)** | Fog of war and private treaties via per-civilization permission groups | "Nobody (operator, rivals, API) can read your army or treaty before the rules reveal it; the enclave is attested" | Toggle between the attacker's view and the attested hidden state; reveal at the battle tick | **Core (second).** Prototype tick-time visibility on day 1; fall back to a simple scouting reveal. |
| **Stablecoins (USDC vault)** | The season purse: entry and market fees split by the program; Merkle claim at season end | "Purse = Σ inflows × rule; the payout rule was committed pre-season; winners claim without operator discretion" | Live purse meter; winner clicks Claim; USDC arrives | **Core.** Swap mock units for devnet USDC through the existing Season PDA path. |
| **AI agents (equal players)** | One onchain action budget per civilization per tick, the same fog view for everyone, plus committed decision digests | "Agents had no speed or information advantage; each agent committed its rationale and policy before the outcome, and the revealed log matches" | Spectator timeline: agent rationale hash → reveal → ✓; leaderboard mixing human and agent civilizations | **Core.** Ship the HTTP/MCP agent API plus one reference agent (any runtime). |
| **x402** | Permissionless agent entry: a 402 challenge pays straight into the vault PDA, then SIWX for per-tick auth | "An autonomous agent joined with no account or human, and its fee went directly into the program vault" | `curl` → 402 → paid → new civilization appears → purse grows | **Include (small scope).** Entry + SIWX only. Stretch: peer-to-peer intel sales between agents. **Cut** operator-sold intel (pay-to-win). |
| **ZK (SP1/Groth16)** | Season settlement proof: replay the event log under `ruleset_hash` to produce the outcome and claim root, verified on Solana | "Payouts follow provably from the logged game under published rules" | "Finalize" button → proof verified → claims unlock | **Defer.** Ship an open deterministic replay verifier and present SP1 as the roadmap. Don't pitch "ZK fog of war" if PER does it. |
| ZK Compression / confidential transfers / zkML / TEE inference | None in the core loop | — | — | **Cut for the hackathon.** Revisit Light Compression at scale and TEE inference for "certified model" leagues. |

### Recommended pitch framing

Don't say "we use five technologies". Say: **"A civilization game where the only fair referee is the chain: humans and agents under one action budget, hidden information enforced by attested hardware, and a prize purse no one can touch."** Each technology is then exercised by gameplay, and each claim is checkable on Explorer. That is how the "leading indicator" story holds up under Colosseum's criteria: functionality, novelty, UX from blockchain, and composability, since any agent from any framework can enter.

### Top risks to manage this week (18 days left)

1. The civilization simulation is **offchain and not synced** today. Move the tick onto the ER before anything else.
2. The devnet deployment was blocked by faucet rate limits. Fund disposable devnet keys early.
3. PER visibility computed at tick time is unproven for this team.
4. Session keys aren't implemented, and without them human UX will be full of wallet pop-ups.
5. The legal framing of an entry-funded pool. Keep the demo on devnet and present sponsor-first seasons in the business plan.

### Uncertainties flagged

- No MagicBlock/x402/ZK-specific World's Fair bounty could be confirmed.
- Whether x402 v1.0 has shipped.
- The Solana "Transaction v1" activation date (single source).
- Arcium's status beyond alpha.
- BOLT deprecation.
- x402 volume figures, which conflict between sources and are partly non-organic.
