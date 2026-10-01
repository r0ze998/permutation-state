# Old-game AI inventory, and what ports to Wylls

Research note for the AI-agents plan (owner request 2026-10-01). Written 2026-10-01.
Repo read at `codex/frontier` `aae8617` (worktree `.claude/worktrees/frontier-integ`); run records read from the main checkout's `permutation-gateway/.local/` (the worktree has none). Read-only: no repo change, no API call, no server, no chain transaction. The API key file and the `keys/`, `session.json`, `wallet.json` and `operator-token` files were not opened.

Labels: **[source]** = read in code, a doc or a recorded file (path given); **[measured]** = computed here locally from the repo's own modules with Node, no network; **[estimate]** = arithmetic on stated inputs; **[assumption]** = a choice of input that is not measured. Prices are Anthropic's public pricing page, read 2026-10-01 (`platform.claude.com/docs/en/about-claude/pricing`).

---

## 0. Summary

- The old game (v9, "Wylls" V5) had **five AI pieces**. Two call a language model: the **reference LLM agent** (`agents/llm-agent.mjs`, Claude Sonnet 5 through tools, paid by whoever runs it) and the **advisor** (`src/advisor.mjs`, Claude Haiku 4.5 rephrasing the operator AI members' chat replies, paid by the operator). Three call none: the **rule agent** with its runner, the **MCP server and SDK**, and the operator's own **hosted AI members** (Rust `permutation-server/src/bots/`, rule-based).
- **We have only one real token measurement:** tick 0 of one local season (2026-09-25) used about 11k input tokens and 478 output tokens on `claude-sonnet-5`. After that the API credit ran out [source: `IMPLEMENTATION_STATUS.ja.md` l.33]. No full season was played with a model. Nothing under `.local/` logs per-tick token counts: the agent printed them to stdout only. The one LLM run directory (`.local/agents/sophia-llm/decisions-1790289740751.json`) records four "no decision in time; holding" rationales at ticks 12–13 [source]. Only one chat message was ever recorded in any season file (`e2e-wallet.json`, from a human). No advisor reply was ever recorded [source].
- **Cost of one LLM agent, as the old code is written** (Sonnet 5 at $2 / $10 per MTok, cache read $0.20): about **$0.03–0.06 per decision** in a typical case, and about **$0.40 in the worst case** (10 tool steps) [estimate, §2.1]. For a 180-tick old season that is **$5–10 per agent** [estimate]. The default token cap of 2M counted input tokens would stop the agent after about 57–80 of the 180 ticks [estimate].
- **Cost of the advisor:** about $0.001 per reply. It is capped at 200 replies per gateway run, so at most about $0.22 per run [estimate]. It is negligible.
- **The Frontier changes the economics:**
  - A bell is 600 s instead of a 30-s tick, so one LLM call per bell is affordable per decision, but a 28-day season has 4,032 bells [source: DESIGN §2.1].
  - The program allows 30 actions per hour per citizen (burst 60) [source: M1-CONTRACT §5.6, Season layout +304].
  - The relay sponsors 40 transactions per citizen per game day on days 0–6, then 20 [source: M1-CONTRACT §8.3].
  - Together these cap what an agent can do. **An agent that decides 4 times a day costs about $6 per season. Hourly decisions cost about $34. Every bell costs about $200** [estimate, §5.2].
- **What ports almost as is:**
  - The MCP stdio server.
  - The agent loop: validate before accepting, a token cap, holding on fatal API errors, and caching the static prefix.
  - The runner's crash-safe journal of sealed material.
  - The advisor's fallback-first wrapper.
  - The x402 route and client (for M2).
  - The chat Merkle-anchor code.
- **What must be rewritten:**
  - The rules brief, the order reference and the tool list. The v9 orders, offices and nations do not exist in the Frontier.
  - The state summary. There is no `/api/state`: the summary must be built from herald files per citizen and per province.
  - Every preview and validate tool. There is no game server. The tools must call the WASM kernels or the Rust agent library locally.
  - `submit_orders`. It must become one intent per transaction, with the tlock seal made in code, never by the model.
  - Waiting for the next tick. It must use the bell clock.
- **One rule limits what the operator may do:** Shades may not use an LLM to decide anything. DESIGN §7.1 allows an LLM only for chat flavour, which has no game effect. Any LLM agent we sell is therefore a **player's** agent (bring your own key, or a local model), or operator chat flavour. It can never be a Shade's decision-maker.

---

## 1. The old pieces, one by one

### 1.1 Reference LLM agent: `permutation-gateway/agents/llm-agent.mjs` (120 lines) [source]

**What it does.**
- It joins over x402, using the shared runner (§1.3).
- Every tick it sends Claude the full state summary plus 13 tools: all of `tools.mjs` except `get_state`, which it receives up front.
- It loops for at most `--max-steps 10` model calls, and must finish within `secondsLeft − 4` seconds (a tick was 30 s [source: `src/config.mjs` `tickSeconds: 30`]).
- It must end with `submit_orders`. The agent intercepts that call, runs `game.validate`, and accepts the batch only if no held office reports an error. Otherwise the error goes back to the model to fix.
- The model's one- or two-sentence rationale is committed as a hash and revealed after the tick (`decision.mjs`).

**Model and settings.**
- Model: `claude-sonnet-5` by default (`PS_MODEL` overrides it).
- `max_tokens: 2048` per call.
- It calls the HTTP API with raw `fetch` against `/v1/messages`, with no SDK and **no `thinking` parameter**. On Sonnet 5, leaving `thinking` out runs adaptive thinking, so thinking tokens count as output and share the 2,048 cap.

**Caching.**
- `cache_control: ephemeral` (5-minute TTL) is set on the system prompt and on the last tool definition.
- The growing conversation inside a tick is **not** cached.

**Budget and failure handling.**
- `--max-input-tokens` defaults to 2,000,000. The cap counts input + cache read + cache write tokens together.
- On an unrecoverable error (no credit, bad key, permission denied, budget reached) it sets `unavailable` and holds for the rest of the season with an honest rationale. It does not crash.
- Usage is accumulated in memory and printed to stdout per tick. **It is not written to any file.**

**Prompt size** [measured: `research/measure.mjs` in this folder, which imports the repo's `tools.mjs`]:

| Part | Characters | Tokens [estimate] |
|---|---|---|
| System prompt (`RULES_BRIEF` 1,720 + `ORDER_REFERENCE` 4,129 + instructions) | 6,829 | ≈ 1.9–2.4k (prose, 3–3.5 chars/token; Sonnet 5 uses the newer tokenizer, about 30% more tokens than older models [source: pricing page]) |
| 13 tool definitions as sent (JSON) | 4,328 | ≈ 1.4–1.7k (JSON, 2.5–3 chars/token) |
| Built-in tool-use system prompt, Sonnet 5 | — | 354 [source: pricing page] |
| **Static prefix (cached)** | ≈ 11.2k | **≈ 3.7–4.5k** |
| State summary (`summarize()`), synthetic views: tick 0 (1 city, 3 units) / mid game (5 cities, 14 units, 6 proposals) / late game (8 cities, 24 units) | 2,967 / 5,917 / 7,797 [measured: `research/summary-size.mjs`] | ≈ 1.0–1.2k / 2.0–2.4k / 2.6–3.1k |
| Each tool result (capped at 8,000 characters by the agent) | ≤ 8,000 | ≤ 2.7–3.2k; previews of a city or of research typically 0.5–2k [assumption] |

**Check against the one measurement.** At tick 0 the static prefix (≈ 4k) plus the state (≈ 1k) is about 5k per call. Two calls (state → `preview_research` → `submit_orders`) give about 11k, which matches the recorded "about 11k input, 478 output" [estimate vs source]. The figure counts the first call's cache write, because the log adds cache tokens into the total.

**Calls per tick.** It has 13 tools and up to 10 steps. Typical play is 2–5 steps [assumption, consistent with tick 0 using 2].

### 1.2 Rule agent: `agents/rule-agent.mjs` (151 lines) [source]

**What it does.** It is deterministic and uses no model, so it costs **0 tokens**. Each tick it works through a fixed priority list:
1. Research up to 3 techs, in plan order.
2. Accept peace or a non-aggression pact.
3. Take money for peace, and post one capture bounty on the leader.
4. For each unit: found a city, move a settler to the best site, attack when the forecast is ≥ 1.3× in its favour, or bring soldiers home.
5. Keep every city building something.
6. Spend each office's budget. For offices it does not hold, keep the top two orders as proposals.
7. Write a Japanese rationale.

**Server load.** It uses the same previews a human sees. Per tick that is one `state`, one `preview research` when the queue is empty, one `preview unit` per idle unit plus a `preview path`, one `preview city` per idle city, and one `validate`. In mid game that is roughly **15–30 HTTP reads per tick** against the game server [estimate from the code].

**Recorded behaviour** [source: `.local/agent-*/decisions-*.json`]. In four seasons (v6, v8a, ref, w) its last rationales read "nothing urgent for my office; carry the slot over". It played full on-chain seasons and claimed its prizes [source: `SUBMISSION.md`].

### 1.3 Runner: `agents/runner.mjs` (225 lines) [source]

This is the shared loop for both agents.
- **Join:** it gets test USDC from the faucet, then joins over x402. If a registration landed but the confirmation was lost, it recovers the seat from chain by wallet.
- **Every tick:**
  - It waits for the observation root.
  - It runs governance before the batch: propose orders for offices it does not hold (at most every 5 ticks per office), support proposals that still validate, and vote in the vote window.
  - It submits one sealed commitment per held office, retrying 4 times but never on `TickFrozen`, `WrongTick`, 409 or `BatchTooLarge`.
  - It reveals the orders once the reveal window opens.
- **Crash safety:** it journals `decisions-<season>.json` and `sealed-<season>.json` so a restarted agent can still reveal.
- **End of season:** it claims the prize, retrying every 5 s for up to 10 minutes until the season is finalized.

It makes no model calls.

### 1.4 Advisor (operator chat phrasing): `src/advisor.mjs` (53 lines), used from `src/routes/roster.mjs` `POST /talk` [source]

**Flow.**
1. Rust `permutation-server/src/play/talk.rs` `ai_replies()` decides **what** an operator AI member answers, using the nation's temperament:
   - It answers only messages from humans addressed to that AI or to its nation.
   - It sends at most 3 replies per poll.
   - It stays silent if openness is below 20.
   - The text is a fixed Japanese template that names the contract price.
2. That reply goes to the gateway with a `draft` (the incoming text and the traits).
3. The advisor asks **`claude-haiku-4-5-20251001`** to rephrase it in one or two sentences, in the sender's language.

**Settings.** `max_tokens: 160`. 4-second timeout. Budget of 200 calls per gateway process.

**Fallback.** On no key, a spent budget, an error or a timeout it falls back to the server's own sentence.

**Guardrails in the prompt.** Only treasury contracts bind; never promise anything else; never claim to be a person; if asked whether it is an AI, say the roster is revealed at season end.

**Who pays.** The operator, with `ANTHROPIC_API_KEY` or `.local/anthropic-key`.

**Size** [measured]: the system prompt is about 578 characters with a typical fallback sentence, so about 200–300 input tokens with the user message. Output is at most 160 tokens.
- Haiku 4.5's minimum cacheable prefix is 4,096 tokens [source: claude-api skill, prompt-caching table], so caching is impossible here. It is also unnecessary.

**Use in practice.** No recorded season contains an AI reply. Only one message was ever sent, by a human, in `e2e-wallet.json` [source]. Tests mock it (`test/advisor.test.mjs`).

**What V5 planned beyond this** [source: `PERMUTATION_STATE_GAME_DESIGN_V5.md` §18.8]: rules for routine play; the model consulted only on contract offers, being spoken to, a declaration of war, an election or a lost city; Haiku for talk and Sonnet 5 for strategy; a per-season budget; a two-level memory. Only the Haiku phrasing was built (§18.12).

### 1.5 MCP server and SDK: `client/src/mcp.mjs` (70 lines), `game.mjs` (379), `tools.mjs` (123), `summary.mjs` (53), `decision.mjs` (45) [source]

**MCP server.**
- It speaks JSON-RPC 2.0 over stdio, one message per line, and implements `initialize`, `ping`, `tools/list` and `tools/call`. `instructions` is set to `RULES_BRIEF`.
- It exposes **16 tools**: the 14 in `TOOLS` plus `join_season` (x402) and `wait_for_next_tick`.
- Keys live in `~/.permutation-agent/`. The policy id defaults to `mcp-agent/v1`.
- **The operator pays nothing**: the person's own MCP client (Claude Desktop, Claude Code or any other) pays for its own model use.

**Tools** (`tools.mjs`).
- Each tool is a thin, honest wrapper over `GameClient`.
- **Errors are returned, not thrown**, so the model can read them and correct itself.
- Orders outside the value bounds are refused before anything is sent.
- `submit_orders` reveals in the background.

**SDK** (`game.mjs`).
- **Reads:** `lobby`, `map`, `state` (the whole world, with perfect information), `preview(kind)`, `validate`, `decisionLog`.
- **Joining:** `faucet`, `joinViaX402`.
- **Playing:** `submit`, with one sealed batch per office, the rationale commitment, and up to 3 earlier reveals packed into each batch.
- **Revealing:** `reveal` and `revealWhenOpen`, which polls `/tick` until the reveal phase.
- **Governance and chat:** `gov` (Stand, Vote, Propose, Support, Recall), `talk` (signed with the session key), `messages`, `roster`.
- **End of season:** `claim`.

**Summary** (`summary.mjs`). A compact, token-cheap view of one nation's state, about 1–3k tokens (§1.1).

**Rationale commitment** (`decision.mjs`). `digest = sha256(tick ‖ obs_root ‖ policy_id ‖ rationale_hash)`, with the rationale revealed later. The default policy id is `officer@2`, so the policy does not reveal whether a human or an AI decided.

### 1.6 x402 joins: `src/routes/x402.mjs` (248 lines) and `client/src/x402-client.mjs` [source]

**Flow.**
1. `POST /x402/join` without a payment answers **402** with `PaymentRequirements` (scheme `exact`, network `solana-devnet` or `solana-localnet`).
2. The payment is a Register transaction signed by the wallet and the session key. The gateway, acting as facilitator, checks:
   - exactly one instruction (Register, with no compute-budget instruction), and that this season, its vault and its mint are named;
   - three signers, with the facilitator first;
   - in seasons with AI members, a uniform registration (kind 2, the default deposit, no pre-season votes, standing for 1–2 offices).
3. It co-signs, simulates, sends, and answers `X-PAYMENT-RESPONSE`.

**Amounts.** The entry fee was 10 test USDC: 80% to the pool, 20% to operations [source: `config.mjs`].

**Use.** Agents and people used the same path, which is what made "agents are members on equal terms" true in the old pitch [source: `SUBMISSION.md`].

### 1.7 Chat and its on-chain anchor: `src/talk.mjs`, `client/src/talk.mjs`, `talk-node.mjs`, `crank.mjs` `anchorTalk`, `scripts/verify-talk.mjs` [source]

- **Message format.** A message is ed25519-signed by the session key over `"permutation-rules/talk" ‖ season ‖ tick ‖ member ‖ to ‖ text`.
- **Limits.** At most 280 characters, and 3 messages per member per tick.
- **Anchoring.** The gateway keeps messages in its state (`TalkBook`). Once per tick the crank sends `AnchorTalk`, an ER instruction that logs the Merkle root as `PS_TALK`. `merkleProof` and `verify-talk.mjs` let anyone prove who said what, and by when.
- **No binding.** Words bind nothing. Only `OfferContract` does.
- **Cost:** no model, one crank transaction per tick that had messages.

### 1.8 The operator's hosted AI members (not a model): `permutation-server/src/bots/` (1,530 lines of Rust) [source]

- **Personas.** Warlord, Builder, Diplomat or Scholar, chosen by secret per-season traits (aggression, greed, loyalty, openness).
- **Planning.** `plan.rs` plans one tick; `contracts.rs` handles treasury contracts; `rationale.rs` writes the sealed rationale.
- **Cost:** none. The advisor (§1.4) is the only model they ever touch.

---

## 2. Token use and cost profile

Prices as of 2026-10-01 [source: pricing page]:

| Model | Input | 5-min cache write | 1-hour cache write | Cache read | Output |
|---|---|---|---|---|---|
| Sonnet 5 | $2 / MTok | $2.50 | $4 | $0.20 | $10 |
| Haiku 4.5 | $1 / MTok | $1.25 | $2 | $0.10 | $5 |

- The Batch API takes 50% off, but it is asynchronous and does not fit a 30-s tick.
- The pricing page notes that Sonnet 5's $2/$10 introductory price became its standard price; the planned move to $3/$15 on 2026-09-01 was cancelled.

### 2.1 LLM agent, per decision (old 30-s tick) [estimate]

**Inputs** [assumption unless marked]:
- static prefix S = 4k, cached;
- state D = 2.5k in mid game;
- each extra step adds a ≈ 0.3k assistant turn plus a ≈ 1.5k tool result;
- the conversation inside a tick is not cached (as the code is written);
- the 5-minute cache stays warm because ticks are 30 s apart.

| Case | Calls | Uncached input | Cache read | Output | $ / decision |
|---|---|---|---|---|---|
| Tick-0-like (measured shape) | 2 | ≈ 7k (+ first-call cache write 4k once per season) | 4–8k | ≈ 0.5k [source: 478] | **≈ $0.02–0.03** |
| Typical mid game | 4 | 4·2.5k + (1+2+3)·1.8k ≈ 20.8k | 16k | ≈ 1.2k (adaptive thinking could double it) | **≈ $0.06** (0.042 + 0.003 + 0.012) |
| Worst case (10 steps, 2,048 output each) | 10 | 10·2.5k + 45·1.8k ≈ 106k | 40k | ≤ 20k | **≈ $0.42** |

**Per old season** (180 ticks): typical **$5–10 per agent**; worst **≈ $75** [estimate].

**The default 2M-token cap runs out early.** It counts cache reads as well. A typical tick counts about 25–37k tokens, so the cap would be reached after roughly **57–80 ticks of 180**. The agent then holds for the rest of the season, so the cap needs re-sizing for any real use [estimate].

**Time is also a constraint.** A 30-s tick minus 4 s leaves about 26 s for 2–5 sequential Sonnet calls with adaptive thinking. The recorded holds at ticks 12–13 cannot tell us apart whether time ran out or calls failed: before the fatal-error fix, a credit error also ended in the "no decision in time" text [source: the code path in `decide()`, `IMPLEMENTATION_STATUS` l.33].

### 2.2 Advisor, per reply [estimate]

≈ 300 input tokens × $1/MTok + ≈ 160 output tokens × $5/MTok ≈ **$0.0011 per reply**. The cap of 200 per process gives **≤ $0.22 per run**. Volume is bounded by human chat: at most 3 replies per poll.

### 2.3 MCP server, rule agent, hosted bots, x402, chat

**$0 in model costs to the operator.** The MCP user's own client pays its own model use, at about the same token shape as §2.1 (same tools, same summary), plus whatever overhead that client adds.

### 2.4 What the old pieces never measured (gaps)

- Per-tick usage was never written to a file. It went to stdout only.
- No full season was played with a model.
- No advisor reply was ever sent in a real season.
- The quality of the LLM agent's play against the rule agent was never compared.

If the plan wants these numbers, it should budget a small, owner-approved run (§6).

---

## 3. The Frontier interfaces an agent would use (M1 as built) [source: `docs/frontier/m1/M1-CONTRACT.md` §8, §5.6; code under `permutation-gateway/client/src/frontier/`, `frontier-node/crates/agents/`]

### 3.1 Reading: the herald (`frontier-herald`, §8.4)

| Path | What it holds |
|---|---|
| `GET /h/season` | Season record: genesis time, `bellSecs`, the drand chain info, tip and fee figures, quotas |
| `GET /h/me/{wallet}` | The citizen record, up to 3 holdings, their hosts, transits, open slots, seal codes, and the relay quota |
| `GET /h/province/{P},{Q}/{bell\|latest}` | A JSON envelope with the province account bytes plus the arrival slots, arrival day and clash inputs |
| `GET /h/overview/{ring}/{bell}.bin` | 24 bytes per province: owners, site states, hosts per faction, flags |
| `GET /h/clash/...` | Clash reports |
| `GET /h/events?after=` | Event records, in pages |
| `WS /h/ws` | Live subscriptions to provinces, rings, a wallet or bells |

- **The herald is never a trust root.** Clients decode the account bytes themselves.
- **JS SDK:** `herald.mjs` has `HeraldClient` and the envelope and overview decoders; `codec.mjs` decodes accounts from the ABI vectors.
- **Rust:** `frontier-agents::obs` builds an `Observation` from the same files.
- **Measured raw sizes** [measured: `frontier-node/crates/agents/fixtures/herald-recorded/`, recorded at bell 107 of an in-process day]:
  - `/h/me` ≈ 3–4.5 KB (median 2.9 KB);
  - a province envelope ≈ 5.7–5.9 KB;
  - the season record 4.2 KB;
  - an overview ring of 176–320 bytes.
- **A raw 7-province neighbourhood (one March) plus `/h/me` is ≈ 45 KB.** Most of it is base64, so roughly 15–20k tokens [estimate]. **It must be decoded and summarised before any model sees it.**

### 3.2 Writing: the relay (`permutation-gateway/src/frontier/`, §8.3)

**Routes.**
- `GET /f/relay` gives the fee payer (drawn from a 150-key pool), a blockhash and the quota.
- `POST /f/relay` takes one player instruction per transaction, with an exact compute-budget prefix (`shapes.mjs` allowlist).
- `POST /f/join` handles joins (with invites in the playtest preset).
- `POST /f/reveal` and `POST /f/nudge` are forwarded to the keeper.
- `GET /f/tx/{sig}` reports a transaction's state; `GET /f/quota` reports the quota left.

**Limits.**
- **40 sponsored transactions per citizen per game day on days 0–6, then 20.** Burst 60.
- Per-IP limits apply. Loopback bots are exempt from the IP limits but not from the quotas.
- On chain, every player instruction also pays from an **action bucket of 30 per hour, burst 60** (§5.6 step 3).

### 3.3 Session key

Join carries a session public key and its expiry. `SetSession` sets a new one, at most 30 days ahead. The program accepts `actor == session` until it expires. This is the same shape as the old session key.

### 3.4 Sealed marches (DESIGN §6.2; `seal.mjs`; the web `seal-worker.mjs`; Rust `fclient::seal`)

1. **Build the plaintext** (37 bytes): `version ‖ host_id ‖ arrive_bell ‖ dest P, Q, tile ‖ stance ‖ retreat_bps ‖ path_len ‖ 12-byte path`.
2. **Seal it.** Draw a random 16-byte key `k`. Then:
   - salt = `sha256("PS-SALT" ‖ k)`;
   - commit = `sha256("PS-FRONTIER-MARCH-v1" ‖ plain ‖ salt)`;
   - the 165-byte seal is tlock IBE to drand quicknet round `T(arrive)` plus a 37-byte keystream body.
3. **Depart** with the seal, the commitment and a tip of at least `tip_min`. The plaintext and salt must be **journalled before signing**.
4. **Reveal.** Either the owner reveals in the arrival bell, or a keeper reveals after `T(b)` (anyone can, through the seal).
5. **SettleTransit**, sent by keepers, is where a bad seal is proven.

**Penalties.** An unrevealed march is routed: it loses 50% of its troops. A bad seal destroys the host.

**Node can seal.** The browser seal worker is pure JS (noble bls12-381) and can be imported from Node [source: the header of `permutation-server/web/frontier/seal-worker.mjs`]. The Rust bots use `fclient::seal`.

### 3.5 Clock (DESIGN §2.1, M1-CONTRACT §5.1)

`bell_start(b) = genesis_ts + 600 b`. One day is 144 bells; a season is 28 days (4,032 bells). The earliest arrival is the departure bell + 2. A clash report arrives about 31–41 minutes after departure [model, DESIGN §2.3].

### 3.6 Existing bots (`frontier-node/crates/agents`, `bots`; §8.6)

- **Policy.** `policy::decide` is deterministic in (seed, observation, memory). It turns an observation into intents: Join, FileTicket, Harvest, Build, Train, Muster, Explore, SettleExplore, Depart, Reveal, SettleTransit, Nudge, plus the adversarial personas.
- **Observation.** Only from the herald.
- **Writes.** Every write goes through the relay.
- **Pacing.** Within the 30-per-hour bucket.
- **This is the Shade-shaped policy.** It is also the natural "rule agent" for the Frontier.

### 3.7 Not in M1

- Talk or chat (no route; the relay only reuses the ed25519 helpers from `talk-node.mjs`).
- Governance (M3).
- Money and x402 (M2: DESIGN §2.2 says joins reuse the gateway's x402).
- Rationale commitments: the Frontier program has no decision-digest field.
- Previews and dry runs from a game server: there is no game server; the WASM kernels and the Rust library run locally.

---

## 4. Port or change, piece by piece

| Old piece | What carries over | What must change for the Frontier | Effort [estimate] |
|---|---|---|---|
| **MCP server** (`mcp.mjs`) | The JSON-RPC stdio loop, `initialize`/`tools/list`/`tools/call`, errors returned as data, a key directory, an `instructions` text | New tool set (below); `join_season` becomes `/f/join` (M1, free) and x402 later (M2); `wait_for_next_tick` becomes `wait_for_bell` (bell clock from `/h/season`, or a WS `bells:true` subscription) | Small: the protocol part is about 70 lines, reusable as is |
| **Tools** (`tools.mjs`) | The pattern: thin honest wrappers, value bounds checked before sending, results trimmed to fit a context | **Everything else.** `RULES_BRIEF` and `ORDER_REFERENCE` describe v9 (offices, nations, 24-order batches, Star Gate). New tools are per intent: `get_me`, `get_neighbourhood(radius)`, `list_targets` (`policy::targets` / `stay_targets`), `plan_march(host, target, stance, retreat)` (path via `path.rs` or WASM, seal in code), `harvest`/`build`/`train`/`muster`/`explore`, `file_ticket`, `quota`, `march_status`. **The model never sees seal bytes, salts or tips**: the tool chooses `tip_min` or a preset | Medium |
| **Summary** (`summary.mjs`) | The idea: a compact JSON of what matters, nothing invented | Built from `/h/me` + the province envelopes around the citizen's holdings + the ring overview, decoded with `codec.mjs`. Per citizen (≤ 3 holdings, hosts, transits, open slots, seal codes, quota left), not per nation. Target ≤ 2–3k tokens [assumption] against ≈ 15–20k raw | Medium |
| **Previews and validate** (`game.preview`, `game.validate`) | The role: tell the model facts instead of letting it guess | No game server. Run the WASM kernels (`frontier-wasm`, the same ones the web practice mode uses) or the Rust `frontier-agents` (`path`, `policy::shield_refuses`, `muster_room`, `queue_free`) locally; or simulate the transaction through the relay (the relay already simulates for its drain guard) | Medium |
| **LLM agent loop** (`llm-agent.mjs`) | Validate before accepting; feed errors back; token cap; fatal-error hold; a static prefix cached with `cache_control`; the rationale kept and shown | Bell cadence (one decision per session, not per bell; see §5); use the SDK rather than raw `fetch`; count cache reads separately in the cap; **1-hour TTL or an accepted write per decision** (the 5-minute cache expires between 10-minute bells); size `max_tokens` for adaptive thinking or set effort; one intent per transaction against the quota | Small to medium |
| **Runner** (`runner.mjs`) | Crash-safe journal (the Frontier needs plaintext + salt journalled before Depart, the same idea as `sealed-*.json`); retry rules by error code; claim at the end (M2) | No office governance in M1; owner reveal or keeper reveal through `/f/reveal`; SettleTransit is keeper work; tickets and finality; dormancy | Small; the Rust `frontier-bots` runner already does this for the bots |
| **Rule agent** | The concept of a free, model-less baseline the LLM agent is compared with | Already exists as `frontier-agents::policy` (Rust). A JS port is optional; the MCP tools can call the Rust or WASM logic | None |
| **Rationale commitment** (`decision.mjs`) | The hashing and reveal scheme, a selling point ("what the AI claimed to think, fixed before the outcome") | No field in the Frontier program; the Depart data is fixed at 219 bytes. Options: an off-chain signed log anchored like chat (below), or a later program field (M3+). Not required for play | Small (off-chain) |
| **Advisor** (`advisor.mjs`) | Fallback first, a budget, a timeout, "never claim to be a person"; allowed by DESIGN §7.1 for Shades ("chat flavour") | Nothing to phrase until chat exists (M3). The decision of what to say stays in the committed policy; the model only phrases. Use the model id `claude-haiku-4-5` (the code pins a dated id) | Small, after chat exists |
| **Chat + anchor** (`talk*.mjs`, `anchorTalk`) | Signed bytes, per-bell limits, the Merkle root and proofs, `verify-talk` | `AnchorTalk` was a v9 ER instruction and has no Frontier counterpart. Anchor per bell another way (herald-published root plus an on-chain memo, or a new instruction in M3); retune `TALK_PER_TICK` to per bell or per hour | Small to medium (M3) |
| **x402** (`routes/x402.mjs`, `x402-client.mjs`) | The 402 → signed program instruction as payment → co-sign → settle flow, the problem list, uniform registration | M2 money: the payment becomes the Frontier Join with the citizen fee (and optional laurel stake), escrowed until the first holding is final (DESIGN §2.2); network `solana` on mainnet; the relay pool as fee payer | Medium (M2 scope) |

### 4.1 Frontier rules an agent design must respect [source: DESIGN §2.7, §7]

- **Shades: no LLM may decide any action.** A Shade's policy is open-source code, committed by hash at genesis and replayed by the verifier. An operator-run LLM player would also have to be either a Shade (forbidden) or an undisclosed operator wallet. **So operator-paid LLM agents cannot play.** They can only explain, coach or phrase. Players' agents can play with their own keys or models.
- **Automation is priced to roughly break even or lose.** In the M0/K3 simulator, an SDK-default scripted wallet returns 0.93–0.96× what it paid, against 0.96 for a very skilled human. **An honest pitch for AI agents is "they play for you and save your time", not "they make money".**
- **Equal tools (P6).** The public bot SDK gives everyone the same tools. An MCP server for the Frontier fits this pillar, as the old one did.

---

## 5. What the Frontier means for cost

### 5.1 Decision cadence is the cost lever, not the price per call [estimate]

At **$0.03–0.06 per decision** (the §2.1 shape: a ≈ 4k static prefix plus a ≈ 2.5k summary, 2–4 calls, Sonnet 5):

| Cadence | Decisions per 28-day season | $ per agent per season |
|---|---|---|
| Every bell (10 min) | 4,032 | $120–240 |
| Hourly (the bucket rate; "SDK default, online every hour" [source: DESIGN §2.7]) | 672 | $20–40 |
| 4 sessions a day (the "very skilled" archetype) | 112 | $3.4–6.7 |
| 1 session a day (the "daily" archetype) | 28 | $0.8–1.7 |

### 5.2 The cache gap between decisions

- Decisions are ≥ 10 minutes apart, so a 5-minute cache entry is gone by the next one.
- Each decision therefore pays a cache write on the static prefix: 4k × $2.50/MTok = $0.01 more per decision.
- A 1-hour TTL ($4/MTok write) pays off only if decisions come less than an hour apart [estimate from source multipliers].

### 5.3 The quota caps any agent

- The relay sponsors 40 transactions a day on days 0–6, then 20. A Depart is one transaction; Harvest, Build, Train, Muster and Explore are one each.
- An agent cannot usefully act more than about 20–40 times a game day.
- **A design with one model call per intended action, under that cap, keeps the per-season cost in the "4 sessions a day" row.**

### 5.4 Who pays

- **Players (bring your own key, or MCP):** the operator pays $0.
- **An operator-hosted coach or narrator:**
  - Haiku-class at ≈ $0.001 per short reply [estimate §2.2].
  - A daily per-citizen digest of ≈ 3k input and 300 output tokens on Haiku 4.5 costs ≈ $0.0045 per citizen per day, ≈ $0.13 per season [estimate; assumption on the sizes].
  - At 10,000 citizens that is ≈ $1,300 per season [estimate]. Bounded by a budget, as the advisor was.

---

## 6. Open items for the plan

1. **Measure before promising numbers.** A one-hour, owner-approved Sonnet 5 run over the old 30-s game, or better a Frontier MCP prototype on the local stack, with usage written to a file. Today only one measurement exists: tick 0, ≈ 11k input and 478 output.
2. Decide whether the Frontier pitch is **"your agent plays your citizen"** (players' keys or local models, MCP plus SDK, equal tools) or **"the operator's AIs talk"** (Shade chat flavour, M3). DESIGN §7.1 rules out "operator AIs decide".
3. Re-size the token cap (§2.1); set thinking and effort explicitly; use the SDK; log usage to a file.
4. Rationale commitments are a strong "AI you can audit" feature in the old game and have no Frontier home yet (§4 table).
5. **Local models** (the owner's "Gemma 4" question): out of scope for this note. The tool layer above is model-neutral, since MCP and the summary do not depend on the model, so a local model plugs in at the loop.

## 7. Files read

- Old game:
  - `permutation-gateway/agents/{llm-agent,rule-agent,runner}.mjs`
  - `permutation-gateway/src/{advisor,talk,config,server}.mjs`
  - `permutation-gateway/src/routes/{x402,roster}.mjs`
  - `permutation-gateway/client/src/{mcp,game,tools,summary,decision,talk,talk-node}.mjs`
  - `permutation-server/src/play/talk.rs`, `permutation-server/src/bots/mod.rs`
  - `IMPLEMENTATION_STATUS.ja.md`, `SUBMISSION.md`, `PERMUTATION_STATE_GAME_DESIGN_V5.md` §18.7–18.12
- Frontier:
  - `docs/frontier/DESIGN.md` §1, §2, §6.2, §7, §12
  - `docs/frontier/m1/M1-CONTRACT.md` §0, §5.6, §8
  - `docs/frontier/SUMMARY.ja.md` (the hidden-AI passages)
  - `permutation-gateway/client/src/frontier/{herald,seal}.mjs` (headers and exports)
  - `permutation-server/web/frontier/seal-worker.mjs` (header)
  - `frontier-node/crates/agents/src/{lib,policy,obs}.rs` (headers and the Intent enum)
  - the recorded herald fixtures (sizes only)
- Run records: `outputs/permutation-gateway/.local/{agents/*,agent-*}/decisions-*.json`, `member.json`; the `talk` arrays of the season JSON files.
- Local measurement scripts (this folder): `measure.mjs`, `summary-size.mjs`. They import the repo's modules read-only and make no network calls.
