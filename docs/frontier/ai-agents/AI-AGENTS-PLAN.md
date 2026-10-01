# Wylls: the AI-agent offering (plan)

- **Date:** 2026-10-01. **Status:** revision 1, after review (the review's 14 issues and how each was handled are in §11). Plan for the owner's review. Nothing here is built.
- **Owner request (2026-10-01):** make AI agents a real selling point of the game while watching costs; can an open-weight model such as "Gemma 4" do it?
- **Inputs:**
  - `research/models-prices.md` (models, prices read 2026-10-01, open-weight licences, Mac speed, determinism);
  - `research/old-game-inventory.md` (the old game's AI pieces and what ports);
  - `docs/frontier/DESIGN.md` rev 3.1 (§1 goals, §2 game loop, §5.4 bot criterion, §6.4 threat table, §7 Shades, §9 verification, §12 milestones);
  - `docs/frontier/SUMMARY.ja.md`; `docs/frontier/m1/M1-CONTRACT.md` v1.13 (§5.6 prologue and bucket, §8.3 relay, §8.4 herald, §8.6 bots);
  - the worktree at `codex/frontier` `aae8617` (M1 formally complete).
- **Rules kept in this step:**
  - read-only on the repo;
  - no paid API calls (the key file was not opened);
  - no downloads or installs;
  - no chain transactions;
  - public web pages read: the Anthropic Usage Policy, the Anthropic token-counting docs, EU AI Act Article 50, Apple's Mac Studio page (no price shown); in revision 1 also OpenRouter's Zero Data Retention guide and two public articles on Eternum's Daydreams agents (§10).
- **Labels:**
  - **[source]** = read in a cited page or file;
  - **[estimate]** = my arithmetic on stated inputs;
  - **[assumption]** = an input I chose, to be replaced by a measurement;
  - **[design]** = a choice this plan makes.

  Prices are USD, read 2026-10-01 unless stated otherwise. The cost model is `model/cost_model.py` in this folder, and revision 1 adds `model/revision_r1.py` (sensitivity, price ratios, capped configurations; it imports the first). Both are plain Python with no network, and every number in §5 comes from them.

---

## 0. Answers in short

1. **Gemma 4 exists.** Google released it on 2026-03-31 (E2B, E4B, 26B A4B MoE, 31B dense) and released a 12B "Unified" model on 2026-06-03. All of them are under **Apache 2.0** and have native tool use [source: Gemma 4 model card, Hugging Face API, via research]. **Gemma 4 26B A4B** (about 16 GB at 4-bit; 70–100 tok/s estimated on the M4 Max) is the first open model to try [estimate].
   - **What it can do:** players' agents; fallback commentary; the companion's fixed-menu answers; the lines of *openly disclosed* AI characters.
   - **What it cannot do:** decide Shade actions (no model may, DESIGN §7.1), or write the chat of hidden Shades (revision 1: templates only, §3.5). A reproducible LLM path is possible only on a pinned stack, and it has not been proven (§6.2).
   - **"Free" needs hardware.** Running it locally needs roughly a 32 GB+ Apple-silicon Mac or a 24 GB GPU, and a llama.cpp or LM Studio setup [estimate: ~16 GB of weights plus context and the OS]. Gemma 4 E4B (~5 GB) runs on 16 GB machines but is weaker [estimate]. For most players the cheap path is a hosted open model at cents per season (§5.2), not a local one.
2. **The honest pitch, in two tiers** [design]:

   > **For everyone:** *"An open world of sealed, simultaneous turns. An AI narrator calls every bell, and an AI companion explains your clashes. The operator's Shades are sealed scripted players: their code is locked before the season and anyone can replay it afterwards."*
   >
   > **For builders:** *"An MCP server and SDK with an autonomous agent runner. Your agent plays under the same rules, quotas and sealed orders as everyone else, on your own Claude key or an open model (hosted, or local if you have the hardware). Claude Desktop gives you assisted play while you chat; the runner plays on its own."*

   - What is actually distinctive is **sealed simultaneous turns that agents must reason about** and **operator players that anyone can replay**. "AI agents can play" alone is not: Eternum has marketed agent play since 2025 (Daydreams, "1,000+ agents") [source: Bankless, games.gg, §10].
   - Never claim that an agent makes money. The simulator says an SDK-default bot gets back 0.93–0.96× what it paid, about the same as a very skilled human [source: DESIGN §2.7].
   - Before building the operator-paid parts (F3 free tier, F6), measure demand in the playtest (§7.1).
3. **Who pays.**
   - **Players pay for their own agents' thinking:** their own Claude key, a hosted open model, or their own machine. The operator pays **$0** in model cost for them.
   - **The operator pays only for words:** the companion's free tier, commentary and Shade policy authoring (F8). **The recommended setup costs ≈ $3 / ≈ $46 / ≈ $139 per 28-day season at 100 / 1,000 / 10,000 players**, that is **2.5% / 4.0% / 1.2%** of the operator's 20% share (upper bound), inside the ≤ 5% cap at every size [estimate, §5.4].
   - For comparison, the **Claude-first setup A** (everything on Haiku 4.5, F8 on Sonnet) costs ≈ $38 / $115 / $875, which is 33% / 9.9% / 7.5% of the share and breaks the cap at every size [estimate].
   - The share is an upper bound (day-0 prices, 30% of wallets staking: DESIGN §8.8). So the cap is enforced live, as 5% of the operator escrow actually accrued, and the governor steps down to cheaper models or templates when revenue is lower (§2 G6).
4. **Findings that change the design:**
   - **(a) The Anthropic Usage Policy forbids Claude-voiced hidden Shades** [source, §3.5].
     - It is effective 2025-09-15.
     - It says *"All consumer-facing chatbots, including any external-facing or interactive AI agent, must disclose to users that they are interacting with AI rather than a human"*, at least at the start of each chat session.
     - It also forbids using outputs to convince someone they are talking to a person when they are not.
     - So hidden Shades must not chat in Claude's words. The old `src/advisor.mjs` prompt ("if asked whether you are an AI, say … revealed at the end of the season") conflicts with that policy and must not be ported as it is.
     - EU AI Act Art. 50(1) (AI systems that talk to people must say so) has applied since **2026-08-02** [source]. Hidden AI players who chat therefore need legal review, whatever writes their lines. **Revision 1:** the same question covers players' own LLM agents chatting in M3, so the official kit flags every message it sends as AI-written (G9).
   - **(b) The Mac has only 35 GB of free disk** (926 GB volume, 97% used) [source: `df`, 2026-10-01]. One mid-size model (Gemma 4 26B A4B, ~16 GB) fits. Two do not, and gpt-oss-120b (~63 GB) does not fit at all until space is freed. The owner's Mac is also the wrong host for public traffic without OS-level isolation (§4.1).
   - **(c) Decision cadence, more than price per token, drives the cost of a playing agent, and the per-decision cost is a range.** A Sonnet 5.5 agent deciding every hour costs **≈ $30–95 per season** in tokens (3–6 calls per decision, 400–1,500 output tokens per call with thinking); at 4 decisions a day it is **≈ $5–16**; on Haiku 4.5 at 4 a day **≈ $1.85–7** [estimate, sensitivity, §5.2]. The kit must set effort or thinking explicitly and print the range before it starts; spikes S1/S2 replace the range with a measured central value.
   - **(d) A free-text companion is a channel for players' intentions.** A question such as "should I hit the camp at (4,−2) next bell?" reveals a target before any seal exists. Revision 1 makes the operator-paid free tier **fixed-menu only** (no free text by default) and keeps planning questions on the player's own key or model (§3.3).
   - **(e) An operator-run LLM that plays (F7) conflicts with DESIGN goal 5** ("nobody, the operator included, can steer the result") in any season with money. Revision 1 limits F7 to seasons with no money (§3.7).
   - **(f) The strong-agent fairness check must come before money, not in M3.** Cheap, always-online LLM agents could arrive with M2's x402 joins; the bot criterion with a strong-agent profile becomes an M2 money gate (§6.3).

---

## 1. What can honestly be claimed, and when

| Claim | True today (M1)? | True after |
|---|---|---|
| "AI agents play by the same rules, quotas and buckets as people" | **Yes in the design and in M1.** The bots use the same herald and relay paths and the same 30/h bucket (contract §8.6). But no LLM agent and no MCP server exist for the Frontier yet | F1 (before M2) |
| "Use Claude Desktop or any MCP client for assisted play while you chat" | No (the old MCP server is v9-only) | F1 (before M2) |
| "Run an autonomous agent with the SDK runner, on your key or an open model" | No | F1 (before M2); public SDK in M3 |
| "Agents join and pay like anyone (x402)" | No (M1 has no money) | M2, **after the strong-agent criterion run passes** (§6.3) |
| "An AI companion explains the world and your clashes" | No | F3 v0 (before M2, bring-your-own key); fixed-menu free tier in M2 |
| "An AI narrator calls every bell" | No (the spectator page exists, with no narration) | F4 v0 (before M2) |
| "Sealed scripted players (Shades), replayable after the season" | Designed (DESIGN §7), not built | M2 (roster, replay), M3 (voting) |
| "Shades talk to you in character" | No; **templates only, and needs legal review** (§3.5) | M3 at the earliest, template phrase banks the verifier replays |
| "AI-written chat is always flagged" | No chat yet | M3 (G9: official kit and client) |
| "Agents vs humans" | Only as *declared* agents vs *undeclared* wallets, which include bots | F6 (M3) |
| "AI agents earn money" | **Never say this.** The simulator says automation returns < 1.0× | — |
| "Free local model" without hardware terms | **Never say this alone.** Always next to "needs ~32 GB+ Mac or 24 GB GPU" [estimate] | — |

---

## 2. Ground rules for every AI feature [design]

- **G1. Words, not decisions.**
  - The rule: no operator-run model decides any game action, **except declared showcase agents (F7) in seasons with no money** (demo, playtest, practice). Shades keep DESIGN §7.1: a committed deterministic policy that the verifier replays.
  - What an operator-run model may do: phrase, explain, summarise and narrate, for disclosed AI text only.
  - What players' own agents may do: decide for their own citizen, through the same public tools as everyone else.
- **G2. The data boundary.** An operator-run model sees only four input classes:
  1. finalised, published herald files (`/h/season`, `/h/overview`, `/h/province/*/{bell}`, `/h/clash`, `/h/events`);
  2. the requesting player's own public records (`/h/me/{wallet}`, which is public chain data anyway);
  3. static rules text;
  4. **player free text** (revision 1), only where a feature accepts it (BYO modes, and the operator free tier only if the owner turns free text on, §3.3). Rules for this class:
     - never written to any log: the usage log keeps tokens, cost, feature and a salted per-day user hash only;
     - ai-gate keeps no copy (retention 0);
     - sent only to a provider route with **zero data retention** or to the self-hosted model [assumption: Anthropic offers ZDR only by agreement; OpenRouter can enforce ZDR routing per request, source §10];
     - the client warns before sending when a question contains coordinates, holding or province ids, and shows a standing banner: "Do not type targets. The operator's server sees your question.";
     - the privacy notice names every processor per provider (§7, M2 item).

  It **never** sees:
  - seal plaintexts, salts or seal keys, the player's own included;
  - the keeper's reveal queue or its journals;
  - relay request bodies;
  - session or wallet keys, invite tokens or the operator token;
  - the Shade roster, leaves, parameters or seeds;
  - API keys.

  How it is enforced:
  - **At the OS level, not just as a separate process** (revision 1). ai-gate and any model server run as a separate OS user, VM or container with no read access to the repo's `.local/`, the keeper's journals, key files or wallet material. They read only the herald over HTTP. On the owner's Mac this means a separate macOS user (§4.1).
  - A CI test greps every prompt the service builds for the forbidden fields, and checks that no request body is written to disk.
  - Why this matters: the keeper does see owners' plaintext early. In-bell owner reveals post plaintext to `/f/reveal`, and the keeper builds the Reveal (contract I-24).
- **G3. Same rules for agents.** There is no special API, no extra quota and no faster path. Agents read the herald and write through the relay or their own RPC under the same program checks: the 30/h bucket with burst 60, and the relay quota of 40 per day on days 0–6, then 20 [source: contract §5.6, §8.3]. **Revision 1:** the official kit's event-reactive behaviour is limited to what every player gets as standing behaviours and alerts (§6.3).
- **G4. Fallback first** (from `advisor.mjs`). Every operator text feature has a deterministic template that ships first. The model only rewrites it. If there is no key, the budget is spent, there is an error or a timeout, or the output fails validation, the template is shown.
- **G5. Facts come from code.** Every number, name, coordinate and bell in operator text is a slot filled by code. A validator rejects output that contains a number or place not present in the input facts, and the template is shown instead. This stops hallucinated commentary from misleading players.
- **G6. Hard budget caps in three layers:**
  1. a per-feature daily USD budget computed from each response's `usage` field;
  2. a per-season ceiling for all operator AI spend: **≤ 5% of the operator escrow actually accrued so far** (re-computed daily, so it follows real revenue, not the upper-bound share), or a fixed owner-set amount in seasons without money. When the ceiling is near, the governor steps down in a fixed order: live commentary to the hosted or self-hosted open model, then companion answers to templates, then commentary to templates [design];
  3. the provider account's own limit: a separate key or workspace per feature and prepaid credit only [assumption: check that the Console's per-workspace spend limit exists for this account].
- **G7. AI is always disclosed** wherever a model writes text that people read: companion, commentary, showcase agents and openly disclosed AI characters. Hidden Shades never chat through a model (§3.5).
- **G8. Models are configurable.** No dated model id is hard-coded. `claude-haiku-4-5-20251001` is "not sooner than 2026-10-15" for retirement; with at least 60 days' notice the earliest is about 2026-11-30 [estimate from source]. Every feature takes `{provider, model}` from config, with a fallback chain: the configured model, then the other provider class, then the template.
- **G9. AI-written chat is flagged** (revision 1, M3). Every chat message, petition or diplomatic note sent through the official kit carries a protocol-level `ai_written` flag in the signed message bytes, shown in the client next to the message. The official kit has no option to turn it off. The terms of service require automated chat to be flagged; third-party kits cannot be forced, so the rule is enforced by reports and the M4 legal review decides what else is needed.
- **G10. Agents run in a dedicated session** (revision 1). The official autonomous runner is its own process with only Frontier tools. The MCP docs warn users not to combine the Frontier server with filesystem, shell, mail or wallet tools in the same session. The session key file lives outside the working directory and outside any path the user's MCP filesystem tools are likely to expose (default `~/.frontier-agent/keys`, mode 0600) [design].

---

## 3. The features

Each feature lists what it is, why it sells, its architecture (where the model runs, what data it sees, rate limits, caching, budget and hard caps, abuse and prompt-injection defences, failure modes) and its milestone. Costs are in §5.

### 3.1 F1 — Bring-your-own agent kit: Frontier MCP server, agent SDK, reference agents

**What it is.** A package (`permutation-gateway/client/src/frontier/agent/` [design]) with five parts:

1. **Summariser.** It turns the herald into a compact JSON view of one citizen. The inputs are `/h/me`, the province envelopes around the citizen's holdings, and the ring overview, decoded locally with `codec.mjs`. The target is ≤ 2–3k tokens against ≈ 15–20k for the raw files [estimate, research §3.1]. **Revision 1, herald spot-check:** the herald is "never a trust root", so before any write the kit compares the citizen's own accounts (Citizen, Holdings, transits) read from a chain RPC with the herald's `/h/me`, and for any clash it reasons about it recomputes the outcome from `inputs_b64` with the WASM kernel and checks `outcomeDigest` (against the chain's recorded result where the program stores one [assumption: confirm the field in the contract]). On a mismatch it refuses to act and logs why.
2. **Tool layer** (model-neutral):
   - **Reads:** `get_season`, `get_me`, `get_neighbourhood(radius)`, `list_targets` (a port of `frontier-agents::policy::targets`), `quota`, `march_status`, `explain_clash(bell)`.
   - **Writes:** `file_ticket`, `harvest`, `build`, `train`, `muster`, `explore`, `plan_and_depart(host, target, stance, retreat)`.
   - **Clock:** `wait_for_bell(n)`, using the bell clock or a WS subscription with `bells: true`.

   How the tools behave:
   - They return errors as data, as the old `tools.mjs` did.
   - They refuse out-of-bounds values before sending anything.
   - Paths come from the WASM kernel or `path.rs`.
   - `plan_and_depart` builds the 37-byte plaintext, seals it in code to `T(arrive)` (the pure-JS `seal-worker.mjs` runs in Node [source, research §3.4]), and journals the plaintext and salt **before** signing. **The model never sees seal bytes, salts or tip amounts:** the tip is a preset, one of the relay's three.
3. **MCP server** (stdio JSON-RPC). The protocol code of the old `mcp.mjs` (~70 lines) is reused [source]. Its `instructions` text is a new Frontier rules brief, plus the G10 warning about combining it with other powerful tools.
4. **Reference agents:**
   - **(a) the rule agent:** `frontier-agents::policy::decide` called through a small CLI, or a JS port. It uses no model and costs $0.
   - **(b) the LLM runner:** the old `llm-agent.mjs` loop rewritten. It validates before accepting, feeds errors back, holds on fatal errors, and is capped in tokens **and dollars**. It has two provider adapters: **Anthropic** (Messages API with prompt caching, **effort and thinking set explicitly** — default: Sonnet 5.5 with up-front thinking off (`between_tools`), Haiku 4.5 with no thinking [source: research §1.1] — usage logged to a JSONL file) and **OpenAI-compatible**, which covers llama.cpp `llama-server`, LM Studio, Ollama and OpenRouter.
5. **Key handling.**
   - The wallet key is used only for Join, `SetSession` and, from M2, x402 payment and Claim.
   - The agent plays with a **session key** (the program accepts `actor == session` until it expires, ≤ 30 days [source: research §3.3]), stored per G10.
   - The MCP server has no tool that exports or reads a key.

**Why it sells.** It is the builders' tier of the pitch: "an MCP server and SDK with an autonomous runner". Two modes, stated plainly [design]:
- **Assisted play:** Claude Desktop, Claude Code or any MCP client plays while the human is chatting with it. It does not act on a schedule. Subscription users pay through their own plan and its usage limits; the operator pays nothing and quotes no price for it.
- **Autonomous play:** the kit's runner (or a headless MCP client the user schedules) plays on its own, on the user's API key, a hosted open model, or a local model on suitable hardware (§0.1).

It costs the operator nothing and fits pillar P6: everyone gets the same tools.

**Architecture.**

| Aspect | Design |
|---|---|
| Where the model runs | On the player's side: their MCP client and their key or plan, a local model on their machine, or a hosted open model they pay for. The operator runs no model |
| Data the agent sees | Whatever the player's tools fetch: public herald data (spot-checked against the chain), and the player's own journal (their own plaintexts, which are theirs). It sees no other citizen's sealed destination, because none exists anywhere before `T(b)`, except the keeper's queue (G2) |
| Rate limits | On chain: the bucket (30/h, burst 60). Relay: 40 sponsored transactions per citizen per day on days 0–6, then 20 (≈ 700 a season [estimate: 7×40 + 21×20]). Herald: per-IP limits, CDN caching, and WS subscriptions of ≤ 64 provinces [source: contract §8.3–§8.4]. The kit prefers the WS and the immutable per-bell files to polling |
| Default cadence | **4 decisions a game day** plus event wake-ups on what is actually public: a clash report on an own holding; **a departure, by any other faction, from a province within N provinces of an own holding, with its origin, arrival bell and departure mass** (destinations are sealed, so "towards an own March" cannot be known [source: DESIGN §6.2 Depart]); a siege horn; the Shield ending. Wake-ups may only trigger the actions every player also has as standing behaviours (§6.3). Configurable, with the printed cost range before start [design] |
| Caching | Static prefix (rules brief + tool schemas) first; the summary next; `cache_control` on the last block of each call, so calls within a decision read the previous context. Haiku 4.5 caches only prefixes ≥ 4,096 tokens [source]; a 7,000-token first call qualifies. Decisions are ≥ 10 min apart, so the 5-minute cache is written once per decision; the 1-hour TTL pays only if decisions come less than an hour apart [source: multipliers 1.25× / 2×] |
| Budget and hard caps | `--max-usd-per-day` and `--max-usd-per-season`, computed from the `usage` fields (cache reads counted separately, unlike the old 2M-token cap that stopped the agent after ~57–80 of 180 ticks [estimate, research §2.1]); `max_tokens` per call; ≤ 6 model calls per decision; ≤ 3 write intents per decision; a quota reserve kept for reveals and settles |
| Safety limits in the tool layer | Never march more than X% of the troops at home in one decision (default 60%); never Dissolve the last host; never spend the last 5 relay quota units; money moves (x402 join, claim) need a human confirmation unless `--allow-spend-usdc <cap>` is set (M2) [design] |
| Prompt injection | M1–M2 data is numbers and bytes from the program: no free text reaches the agent. From M3 (chat, Company names, petitions) every player-authored string reaches the model only wrapped as quoted data (`<untrusted from=… >…</untrusted>`), with length and character limits and control characters stripped, and the tool layer's hard limits hold whatever the model says. **The kit's limits cannot protect tools outside the kit**, so G10 (dedicated session, key file placement, docs warning) is the defence against an injected message telling the model to use filesystem, shell, mail or wallet tools. The red-team corpus includes cross-tool exfiltration cases (spike S5) |
| Failure modes | API down, out of credit or over budget: the agent **holds**, with an honest log line (old pattern), and the kit can fall back to the rule agent. Herald and chain disagree: the agent refuses to act. A crash after Depart: the journal keeps the plaintext and salt, and the keeper reveals through the seal anyway. A malformed seal cannot happen from the model, because the seal is built in code. A rules change (conquest rules come next): tools are versioned by `ruleset_hash` from `/h/season`, and the kit refuses to run against an unknown ruleset |

**Milestone.** F1 v0 before M2, on the local Mode-A stack and, if approved, the private playtest. It is public as "the bot SDK" in M3 (DESIGN §12 lists "public bot SDK" in M3).

### 3.2 F2 — Agents join and pay through x402 (M2)

**What it is.** The old `routes/x402.mjs` flow, ported [source: research §1.6]:
1. A `POST` without a payment gets a 402 with `PaymentRequirements`.
2. The payment is the **Frontier Join**, carrying the citizen fee and an optional laurel stake, escrowed until the first holding is final (DESIGN §2.2).
3. The gateway checks the instruction against the allowlist, co-signs, simulates and sends.

`x402-client.mjs` goes into the agent kit, so `join_season` is one tool.

**Why it sells.** An agent can discover the game, pay its own entry and play with no human in the loop: "agentic commerce" on Solana. Agents and people use **the same** join path, which is what made "members on equal terms" true in the old pitch.

**Architecture notes.**
- No model is involved.
- The kit enforces a **per-season USDC cap** that the human sets, and asks for confirmation above it.
- The relay quotas and the bucket apply as for anyone.
- Sybil agent farms pay the fee per wallet, and the bot criterion (< 1.0× at 1–10% bot shares, best response) is what makes farms unprofitable [source: DESIGN §5.4]. **Revision 1:** that criterion, re-run with a strong-agent profile, is a gate for opening money seasons (M2), not an M3 item (§6.3).

**Milestone.** M2, together with money, after the strong-agent run passes.

### 3.3 F3 — Companion for human players

**What it is.** A panel in the web client (and a JA/EN view in the spectator page) that can:
- explain the state ("why did my host bounce?", "what does Brace do against Flank?");
- explain a clash report in plain words (from `/h/clash`, decoded by code);
- suggest next steps for the daily 15–30-minute session ("you have 3 queue slots free; these 3 camps are ≤ 2 bells away", ranked by code);
- in BYO mode only, draft a march **that the human confirms and seals in the browser**.

**Why it sells.** It lowers the complexity barrier, which DESIGN R3 says is a risk with no Skirmish tutorial, and it is the "AI teammate" that people understand at once.

**Two modes** [design, revised]:

| Mode | Model runs | Who pays | What it may see |
|---|---|---|---|
| **BYO (default for planning questions)** | The player's own key or model. **(i)** A local OpenAI-compatible server at `localhost` (llama.cpp, LM Studio, Ollama), or a small localhost companion proxy the player runs, which holds their key — the recommended route for a key. **(ii)** Direct browser calls with the player's Anthropic key **held in memory only** by default (gone on reload); saving it in browser storage is an explicit opt-in with a warning that any script on the page could read it | The player | Public data **and** the player's own browser journal, including their own march plaintexts and free-text planning questions. The data goes only to the player's chosen provider |
| **Free tier (operator-paid), fixed menu** | ai-gate (§4) on the recommended model per size (§5.4): a zero-data-retention hosted open model, the isolated self-hosted model, or Haiku 4.5 within the cap | The operator | Public data and the player's public records only (G2 classes 1–3). **No free text by default:** the player picks from a fixed menu ("explain my last clash", "why did this host bounce", "what can I do with my free slots", "explain this rule"); code builds the context. For "where could I march", code ranks ≥ 3 candidates from public data and the model explains them; the player picks in the browser. **The operator learns that a player asked for candidates, not which one was chosen** (revision 1: v1's "never learns" did not hold once a player typed a target) |

If the owner later turns on free text in the free tier, every G2 class-4 rule applies (no logging, retention 0, ZDR route or self-host, banner, coordinate warning, privacy notice).

**Architecture (free tier).**

| Aspect | Design |
|---|---|
| Access | Signed in with the player's session key (a signature over a nonce, so only citizens can use it); ≤ 5 questions per citizen per day [design]; per-IP limits as the relay's. The sign-in ties a question to a wallet, which is why free text is off |
| Caching | A static prefix (rules glossary, stance table, doctrine table, examples) padded to ≥ 4,096 tokens so it caches on Haiku 4.5 [source: minimum], with a **1-hour TTL** (written at most once an hour, read at 0.1× [source]). The per-player context (~2k tokens) is not cached. Menu answers that depend only on public, immutable inputs (a clash report) are cached and shared, like F4's explainer |
| Budget | A per-day USD budget inside the G6 ceiling; when it is spent the panel says "the free coach is resting until 00:00 UTC; use your own key or a local model" and shows template explanations |
| Write power | None. The free tier has no tool that writes and drafts nothing |
| Web security (both modes) | A strict Content-Security-Policy (no inline script, `connect-src` allowlist of the herald, relay, ai-gate and the BYO providers the player configured); from M3, every player-written string (chat, Company names, petitions) is rendered escaped, never as HTML; BYO-key theft is in K11 and in the M4 companion red-team |
| Prompt injection | With a fixed menu there is no free text in the free tier. In BYO mode the player's own text goes to their own provider |
| Failure modes | Timeout, error or budget: the template explanation (clash reports and stance tables are already rendered by code). Wrong advice: numbers are slots (G5); every reply carries "AI suggestion, check before you act" |

**Option F3b: a daily dispatch.** A per-citizen morning summary (opt-in, Batch API). The default is a code template; the model only adds one paragraph of prose. Costed separately in §5 and not in the recommended setup.

**Milestone.** BYO mode and template explanations before M2 (no operator spend). The fixed-menu free tier in the M2 beta, only if the playtest metrics justify it (§7.1) and the owner sets a budget.

### 3.4 F4 — Heralds for spectators and the demo: bell crier, clash explainer, recaps, Chronicle

**What it is:**
- **Bell crier:** one to three lines per bell about the notable clashes and events (ring openings, first holdings, big routs), shown on `spectate.html` and in a public feed.
- **Clash explainer:** a plain-language reading of any clash report, generated **once per clash** and cached next to the herald's immutable `/h/clash/{P},{Q}/{bell}`, so every viewer shares one generation.
- **Daily recaps:** one for the whole civilization and one per faction (7 a day), through the **Batch API** at 50% off [source].
- **Season Chronicle:** a long-form history written at the Reckoning from the event log. The Chronicle root already persists across seasons (DESIGN §3.7).

**Why it sells.** It is the demo: a live map with an AI narrator turns a quiet 10-minute bell into a story, and it is cheap because every viewer shares the same text. It is part of the "for everyone" tier of the pitch.

**Architecture.**

| Aspect | Design |
|---|---|
| Where the model runs | ai-gate. Recommended (§5.4): live lines on the hosted or self-hosted open model at 100 and 10,000 players and on Haiku 4.5 at 1,000; recaps and the Chronicle on Haiku 4.5 Batch at every size. Spike S3 checks open-model Japanese before this is fixed |
| Data | Only finalised herald files, after the bell has resolved (`/h/clash`, `/h/events`, `/h/overview`). Destinations are public after reveal anyway, but the crier waits for the resolve so it never "previews" an incoming attack faster for some readers than others. No keeper data, no Shade roster (it cannot say "this is a Shade", because it does not know) |
| Neutrality | **Code picks what is covered**: the top-K clashes by troops involved, with ties broken by `hash(bell, P, Q)` and a rotation so every faction gets coverage. The model only writes. This keeps attention, which is a public good, from being steered |
| Caching | A shared ≥ 4,096-token style guide, glossary and few-shot prefix with a 1-hour TTL, read at 0.1× on every bell. Clash explanations are cached forever (immutable input → immutable output, keyed by `inputDigest`). At ~3 lines an hour (100 players) the governor uses a 5-minute TTL or none |
| Budget | A per-day budget per sub-feature inside the G6 ceiling; the crier is skipped (the template line shows) when its budget is spent; the Chronicle has its own one-off budget |
| Prompt injection | None in M1–M2: there is no player free text. In M3 the crier may quote chat only through the moderation filter, quoted as data, never as instructions |
| Failure modes | A template line per event kind always exists. A wrong number is rejected by the fact validator (G5). A model outage means template lines, and nothing else breaks |
| Disclosure | Each line is marked as written by AI |

**Milestone.** Template crier and clash explainer before M2. They can be built and demoed on the recorded herald fixtures and on Mode-A replays, with no chain and no money. The LLM crier in the M1 playtest if approved (operator budget from the spike), with a with/without comparison of spectator retention (§7.1), and the Chronicle at the first full season.

### 3.5 F5 — The Shades' voice and diplomacy phrasing (M3, needs a legal decision)

**What it is.** When chat exists (M3), the committed Shade policy decides **whether and what** a Shade says: an intent from a fixed menu, such as `greet`, `propose_truce(target)`, `refuse`, `warn`, `boast` or `ask_help`, with slots filled by code.

**Revision 1: templates are the only voice for hidden Shades.** Each persona has a committed phrase bank (hashed into the roster leaf with the policy). The policy chooses intent, slots and the phrase by a seeded RNG, so **the verifier replays the exact text**, not only the intent. A model may not phrase hidden Shades' lines, because:
- an intent tag is replayable but wording is not, and a logged text hash proves nothing about who wrote the text;
- free wording would let the operator hand-write or bias messages that carry more than the intent ("warn" plus "Faction X is about to betray you"), steering human diplomacy while every Shade action still replays cleanly — against DESIGN goal 5;
- an output filter cannot stop this, because persuasion needs no numbers;
- and Claude may not write them in any case (below).

**Provider policy** [source: Anthropic Usage Policy, effective 2025-09-15]:
- The policy requires interactive AI agents to disclose that they are AI at the start of each chat session.
- It forbids using outputs to convince a person they are talking to a human.
- A Shade is meant to be *hidden* until the Reckoning, so **Claude must not write hidden Shades' chat**.
- The same applies to the old advisor prompt, which tells Haiku to deflect "are you an AI?".

**Law.** EU AI Act Art. 50(1) has applied since 2026-08-02 [source]: AI systems intended to interact directly with people must inform them, unless that is obvious from the context. Whether a game-wide notice ("some citizens are operator AIs; any message may be AI-written") is enough, and whether a rule-based template bot counts as an "AI system" at all, are **legal questions** for counsel. They should be asked now, because they shape the Shade fantasy itself. **The same review must cover players' own LLM agents** that chat with humans through the official kit (G9).

**Options, in order of preference [design]:**
1. **Template voice** (the only option for hidden Shades). Fully replayable by the verifier; no model provider's policy involved. Legal review still decides on disclosure.
2. **An open model for openly disclosed AI characters only** (labelled NPC heralds, "faction oracles"): no hidden identity, no game effect. Claude may also be used here.
3. **Forbidden until proven:** an open model phrasing hidden Shades. It could be reconsidered only if the model weights hash, runtime build, prompt and seed are committed, decoding is single-slot greedy, and the verifier reproduces the text bit for bit across machines (spike S4 would have to pass, §6.2).

**Architecture (template voice).**
- **Data:** the policy's intent and slots, the committed phrase bank, the seeded RNG. No model.
- **Output rules:** ≤ 280 characters; no promises outside the binding instruction set; no claim to be human; numbers only from slots.
- **Rate:** ≤ 3 replies per Shade per poll (as `talk.rs`) and ≤ 20 lines per Shade per day [assumption].

**Milestone.** M3 (chat and its anchor are M3). The legal question goes to the owner now.

### 3.6 F6 — Agent registry, badges, leaderboard, agents vs humans (M3)

**What it is.** An optional, signed declaration by a citizen that it is run by an agent. It is off-chain: a message signed by the wallet, published by the herald and anchored with the chat or rationale Merkle root, so it needs no program change [design]. Declarations come in three tiers:

| Badge | What it proves | How |
|---|---|---|
| **Declared agent** | Nothing technical; the owner says it is an agent and names the model and kit | Signed declaration |
| **Committed rationales** | What the agent claimed to think was fixed before the outcome | The old `decision.mjs` scheme (`sha256(bell ‖ obs ‖ policy_id ‖ rationale_hash)`), batched into the per-bell Merkle root and revealed after the clash. The Frontier program has no field for it, so it is off-chain and anchored (≈ 5,000 lamports per anchor transaction [assumption: one memo per bell], ≈ 0.02 SOL per season [estimate]) |
| **Verified policy** | The wallet played exactly a deterministic policy | `policy_code_hash` and seed declared **before** the first action, the code published after the season, and the Shade replay of `frontier-verify` (M2) re-run on that wallet. Only model-free agents can earn it (§6.2) |

**Leaderboard and the agents-vs-humans view:**
- The metric is return per USDC paid and the laurel rank, the same as the join page's table. Raw aggression is never the metric.
- Categories: *declared agents*, *verified policies*, *undeclared wallets*, and, after the Reckoning only, *operator Shades (replayed)*.
- **Stated honestly on the page:** undeclared wallets include bots, because DESIGN §7.4 says many humans run SDK bots. So "agents vs humans" is "declared agents vs everyone else".
- No money and no in-game effect attach to the board: prestige only, so it adds no profitable bot strategy.

**Milestone.** Declarations and the leaderboard in M3, with the public SDK, if the playtest metric on declared agents justifies it (§7.1). The verified-policy replay needs M2's verifier extension.

### 3.7 F7 — Showcase agents (declared, seasons with no money only)

**What it is.** One or two operator-run, **openly labelled** reference agents, for example "Claude plays the Frontier". Each bell, a page shows the agent's committed rationale and its revealed thinking after the clash.

**Where it may run (revision 1).** Only in **demo, playtest and practice seasons with no money** (G1's exception). In a money season, an operator-run LLM that plays changes other players' payouts even when its own claim is barred: it can pick which faction to attack, act as kingmaker or take a faction's arrival slots; its decisions cannot be replayed (§6.2); the operator controls its prompt; and it would run next to the keeper. That conflicts with DESIGN goal 5 and §6.4.

**If the owner still wants it in money seasons** (decision §8.3 (c)), all of these are required, and a verifier item is needed in M2:
- its wallet is listed at genesis as **prize-ineligible** and its claim goes to the pools;
- its inputs are herald files only;
- its full prompt and the transcript for each bell are committed before the clash and published after it;
- it runs on a host with no keeper or relay access;
- its targets follow a fixed, published code rule (for example, a faction rotation chosen by code), so the model chooses only how, never whom.

**Cost.** Operator budget, fixed whatever the number of players: one Sonnet 5.5 plus one Haiku 4.5 agent deciding hourly ≈ **$41 per season** [estimate, plan base shape; up to ≈ $140 at the high end of §5.2's range]. It is counted in no money-season total (§5.4). **Milestone:** the M1 playtest (if approved) and M2 beta demo seasons.

### 3.8 F8 — AI-authored Shade policies (pre-genesis, then hashed)

**What it is.** Before genesis, a model (Claude Sonnet 5.5 or Opus 5.5 offline, or a local model) proposes **parameter vectors and personas** for the committed Shade policy code within bounds that the code validates. Each proposal is scored in `frontier-sim` and must pass the doctrine gate and the bot criterion. The chosen vectors go into the roster leaves; the code hash goes into `policy_code_hash`. Revision 1: personas include the template phrase banks of §3.5, which the model may draft offline before they are committed.

The verifier replays plain code, exactly as today. This makes a true claim possible: "the Shades' strategies were designed by AI, then sealed and replayed", with no model in consensus [source: research §5.2].

**Milestone.** M2 (when the Shade roster is built). It is a one-off cost per season, and parameter sets may be reused across small seasons.

---

## 4. Shared architecture

```
             public chain logs ──► frontier-herald (files + WS, CDN)  ◄── never a trust root
                    │                   │ HTTP only
                    │ RPC spot-check    │
        ┌───────────┼───────────────────┼──────────────────────────────────────┐
        │ players' side                 │                operator's side       │
        │           ▼                   ▼                                      │
        │  agent kit (F1/F2)        summariser (shared JS lib)    AI service "ai-gate" (F3 free tier,
        │   ├ MCP server / SDK          ▲                           F4, F7 in no-money seasons):
        │   ├ tool layer + seal ─────┐  │                           separate OS user / VM, herald only (G2)
        │   ├ journal (plain+salt)   │  │                            ├ prompt builder + fact slots (G5)
        │   ├ session key (G10)      │  │                            ├ budget governor (G6), usage JSONL (no text)
        │   └ model: own Claude key, │  │                            ├ providers: Anthropic | OpenAI-compatible
        │     local llama.cpp, or    │  │                            │   (isolated llama.cpp box, ZDR hosted) | template
        │     hosted open model      │  │                            └ output validator → template fallback
        │                            ▼  │                                              │
        │                     relay /f/relay, /f/join (x402 in M2)                     │
        │                     keeper (/f/reveal) ── holds owners' early plaintext      │
        │                       └─ other host or OS user; never readable by ai-gate ───┘
        └────────────────────────────────────────────────────────────────────────────────┘
```

- **ai-gate** generalises `advisor.mjs`: fallback first, a budget, a timeout, no secrets, and model ids from config. It adds a USD governor from the `usage` fields, a JSONL usage log with no prompt or answer text, separate keys per feature, and a kill switch per feature.
- **The self-hosted open model** is `llama-server` (llama.cpp) with an OpenAI-compatible endpoint.
  - The research prefers the GGUF path for Gemma 4, because MLX may never stop reasoning under LM Studio [source: research S15].
  - A small number of slots for latency (interactive), and one slot for any reproducibility test [source: research §4.1].

### 4.1 Placement and isolation (revision 1)

- **The owner's Mac holds** the Anthropic key file, the deployer and oracle keys, wallet material and, in playtests, the keeper and relay. A process running as the same OS user can read all of them. "ai-gate is its own process" is therefore not isolation.
- **For the owner's own testers only:** the Mac may run `llama-server` and ai-gate, under a **separate macOS user** with no read access to the repo's `.local/`, the keeper's directories or `~/.config`, listening on localhost.
- **For any traffic beyond the owner's testers** (the private playtest with outside testers, public seasons): the same separate user (or a VM or container), the inference port exposed only through an authenticated tunnel to ai-gate, the **keeper on another host**, and log rotation with a disk quota (35 GB free leaves little room for logs next to a 16 GB model).
- **Public seasons:** a zero-data-retention hosted open model, or a dedicated box, not the owner's Mac (§5.4).

---

## 5. Costs

### 5.1 Prices and shapes used

**Prices** [source, Anthropic pricing page and OpenRouter list, read 2026-10-01], in USD per million tokens:

| Model | Input | Output | Other |
|---|---|---|---|
| Haiku 4.5 | 1 | 5 | 5-min cache write 1.25, 1-hour write 2, cache read 0.10 |
| Sonnet 5.5 | 2 | 10 | 5-min cache write 2.5, 1-hour write 4, cache read 0.20 |
| Gemma 4 26B A4B, hosted on OpenRouter (lowest listed) | 0.0765 | 0.255 | — |
| gpt-oss-120b, hosted on OpenRouter (lowest listed) | 0.037 | 0.17 | — |
| gpt-oss-20b, hosted on OpenRouter (lowest listed) | 0.018 | 0.09 | — |

Notes on these prices:
- The Batch API is 50% off and stacks with caching.
- Sonnet 5.5 uses a tokenizer that produces **≈ 1.3× as many tokens** for the same text [source: token-counting docs, "approximately 30 percent more"].
- "Sonnet-class" below means Sonnet 5.5, which costs the same as Sonnet 5.
- **Per list token, hosted open models are 13–29× cheaper than Haiku 4.5** (Gemma 4 26B A4B: 13× on input, 20× on output; gpt-oss-120b: 27–29×; only gpt-oss-20b reaches ≈ 56×) [source: OpenRouter, read 2026-10-01; ratios by `revision_r1.py`]. **On this plan's cached workloads the gap is ≈ 6–8×**, because Haiku's cache reads cost $0.10 (companion at 10,000 players: $735 vs $105; commentary: $128 vs $20) [estimate]. Revision 1 corrects v1's "20–50×", which came from the research summary.
- The hosted-open figures assume the lowest listed price and no cache discount; a zero-data-retention route may cost more [assumption: check per provider in S3].

**Self-hosted box** [assumption / estimate]:
- Hardware: a 128 GB Apple-silicon desktop at **$4,500** [assumption: Apple's page showed no price on 2026-10-01], amortised over 24 months, plus 90 W at $0.20/kWh, is **≈ $185 per 28-day season**.
- Speed: prefill 800 tok/s [estimate from llama.cpp 7B Q4 at 886 tok/s on the M4 Max, source]; generation 80 tok/s for one stream [estimate: Gemma 4 26B A4B at 70–100]; ×2 aggregate at 4 streams [assumption]; 70% usable time.
- On the owner's existing Mac the capital cost is $0 and only electricity counts, but see §4.1.

**Workload shapes** [assumption; all to be replaced by spike S1/S2/S3 measurements]:

| Workload | Input | Output | Count per season |
|---|---|---|---|
| Agent decision (F1, F7), **base** | prefix 4.5k + summary 2.5k; 3 calls, each adding 1k of tool result and assistant turn; incremental caching | 250 per call (Haiku) or 400 (Sonnet) | — |
| Agent decision, **sensitivity** | the same, with 4–6 calls | 400–1,500 per call (the high end is adaptive thinking left on) | — |
| Companion question (F3) | 4.5k cached prefix (1-hour TTL) + 2.1k fresh | 300 | 30% of players use it × 3 questions × 20 active days = **18 per player** (an upper case; the playtest metric replaces it, §7.1) |
| Bell crier (F4) | 4.2k cached + 1.5k fresh | 150 | 2,016 / 4,032 / 24,192 lines at 100 / 1,000 / 10,000 players (half the bells / every bell / 6 fronts per bell) |
| Clash explainer (F4) | 4.2k cached + 2k | 250 | 1.5 unique clashes explained per player |
| Daily recaps (F4) | 4k / 8k / 20k input, Batch | 800 | 196 (7 a day) |
| Season Chronicle (F4) | 60k, Batch | 6k | 1 |
| Daily dispatch (F3b) | 3k, Batch | 300 | 50% opt-in × 28 days |
| Shade line (F5), **reference only** | 2.6k (persona 1.5k + thread + intent) | 80 | Shades = ⌈N/200⌉ (DESIGN §7.1), 20 lines a day each. Revision 1 uses templates (cost $0); the model cost is kept to show what was given up |

### 5.2 What a player pays for a bring-your-own agent (F1), per agent per 28-day season [estimate]

**Per decision, as a range** (`revision_r1.py` section A):

| Shape (calls × output tokens per call) | Haiku 4.5 | Sonnet 5.5 |
|---|---|---|
| Base: 3 × 250 (Haiku) / 3 × 400 (Sonnet) | $0.0165 | $0.045 |
| Old game "typical mid game": 4 × 800 | $0.031 | $0.071 |
| Research W3-like: 5 × 600 | $0.032 | $0.075 |
| Kit cap: 6 × 400 | $0.032 | $0.075 |
| Kit cap with thinking: 6 × 1,500 | $0.065 | $0.141 |

Hosted Gemma 4 26B: $0.002–0.007 per decision. Own Mac: ≈ 15 s and **< $0.0001** of electricity per decision (on suitable hardware, §0.1).

**Per season:**

| Cadence | Decisions | Haiku 4.5 | Sonnet 5.5 | Hosted Gemma 4 26B | Own Mac (electricity) |
|---|---|---|---|---|---|
| 1 a day (the "daily" archetype) | 28 | $0.46–1.81 | $1.26–3.94 | $0.06–0.19 | ≈ $0.002 |
| **4 a day (kit default; the "very skilled" archetype)** | 112 | **$1.85–7.22** | **$5.06–15.76** | $0.23–0.75 | ≈ $0.007 |
| Every hour (the "SDK default bot" archetype) | 672 | $11.09–43.34 | $30.34–94.55 | $1.36–4.47 | ≈ $0.05 |

- **Central value:** not fixed by this plan. The low end is the plan's base shape; the W3-like and kit-cap shapes sit near **$0.075 per Sonnet decision (≈ $50 hourly, ≈ $8 at 4 a day)**; the top is thinking left on. The kit sets effort explicitly (Sonnet 5.5 up-front thinking off by default), which aims at the lower half, and S1 (token counts) and S2 (measured `usage` per decision, with and without thinking) set the central value.
- **Reading:** with a $10 day-0 entry (4 citizen + 6 stake) [source: DESIGN §2.2] and an expected return under 1.0×, an hourly Sonnet agent is a hobby expense of 3–9× the entry. The kit must print this range before it starts.
- **Cross-checks:** the old game's one measurement (tick 0, ≈ 11k input and 478 output on Sonnet 5 ≈ $0.03) is the smallest tick, so it supports only the low end. The old game's estimate for a typical tick, ≈ 25–37k counted tokens and ≈ $0.06 [source: research old-game §2.1], matches the base shape in size (31.2k Sonnet tokens) but used more output; it sits inside this range.
- **Operator cost for F1:** $0 in model cost. Herald egress is ≈ 45 KB per decision [source: research §3.1]. At 10,000 players with 20% running hourly agents that is ≈ 60 GB a season, ≈ $1–6 at $0.02–0.09/GB CDN [assumption]. The RPC spot-check (§3.1) uses the player's own RPC endpoint. Relay sponsorship is the same as for any citizen, within the quotas already budgeted in DESIGN §8.8.

### 5.3 Operator-paid features, per 28-day season [estimate]

The "self-host" column shows how much of one box the feature uses (box-hours per day, after the ×2 batching gain). Its marginal cost is electricity (≤ $5 in every cell); the box's **fixed $185 per season** is shared by every feature (§5.4).

| Feature | Players | Claude Haiku 4.5 | Sonnet 5.5 | Self-host (box-h/day) | Hosted open (Gemma 4 26B, ref.) |
|---|---|---|---|---|---|
| **F3 companion free tier** (≤ 5/day cap; 18 questions per player, upper case) | 100 | $13 | $35 | 0.06 | $1 |
| | 1,000 | $79 | $205 | 0.6 | $10 |
| | 10,000 | **$735** | $1,911 | 5.7 | $105 |
| F3 on Haiku at 6 / 3 questions per player | 1,000 | $30 / $18 | — | — | — |
| | 10,000 | $249 / $128 | — | — | — |
| **F4 crier + clash explainer** (live) | 100 | $11.6 | — | 0.05 | $1.0 |
| | 1,000 | $21.9 | — | 0.13 | $2.7 |
| | 10,000 | $125 | — | 0.9 | $19.5 |
| **F4 recaps + Chronicle** (Batch) | 100 / 1,000 / 10,000 | $0.8 / $1.2 / $2.4 | — | (in the row above) | — |
| F4 all parts (v1 rows, for reference) | 100 / 1,000 / 10,000 | $12 / $23 / $128 | $32 / $60 / $332 | 0.06 / 0.14 / 0.9 | $1 / $3 / $20 |
| F3b daily dispatch (opt-in 50%, Batch) | 100 | $3 | $8 | 0.05 | $0.4 |
| | 1,000 | $32 | $82 | 0.5 | $4 |
| | 10,000 | $315 | $819 | 5.2 | $43 |
| F5 Shade voice: **templates, $0** (model costs shown for reference only; Claude not allowed, open model not allowed for hidden Shades, §3.5) | 100 / 1,000 / 10,000 | ($2 / $8 / $84) | ($4 / $22 / $218) | 0.01 / 0.03 / 0.3 | $0.1 / $0.6 / $6 |
| F7 showcase, no-money seasons only: one Sonnet + one Haiku, hourly (fixed) | any | **$41 combined** (two Haiku $22; two Sonnet $61) | | 0.2 | $3 |
| F8 Shade policy authoring, 120 proposal rounds (one-off) | any | $5 | $12 | (offline) | — |
| F1 bring-your-own kit, F2 x402, F6 registry | any | $0 | $0 | 0 | $0 |

Notes:
- **At 100 players, the 1-hour cache writes are about half of the F4 Haiku cost** (≈ $5.6 of $11.6). At ~3 crier lines an hour, a 5-minute TTL or no caching is cheaper. The governor picks the TTL from the observed rate.
- **The companion is the scale driver** (linear in players, and the 30% usage rate is a guess). Its daily cap, the fixed menu and the model choice are the main cost levers.
- The dispatch is mostly template text; the model paragraph is optional and the first thing to cut.

### 5.4 Operator setups, the cap and the recommendation [design; estimate]

All totals are for **money seasons**, so F7 is not included (§3.7). F5 is templates ($0) everywhere. The share is the operator's 20% share at day-0 prices with 30% of wallets staking, an **upper bound** [source: SUMMARY.ja §8; DESIGN §8.8; the 100-player value scaled]. The cap is 5% of it.

| | 100 players | 1,000 players | 10,000 players |
|---|---|---|---|
| Operator's 20% share (upper bound) | ≈ $116 | ≈ $1,160 | ≈ $11,600 |
| **Cap: 5% of the share** | **≈ $5.8** | **≈ $58** | **≈ $580** |
| **A. Claude-first:** F3 free tier + F4 on Haiku 4.5; F8 on Sonnet | ≈ $38 (33%) — over by $32 | ≈ $115 (9.9%) — over by $57 | ≈ $875 (7.5%) — over by $295 |
| **B. Self-host first:** one isolated box runs F3/F4 (≈ 7 box-h/day at 10,000 with dispatch off, inside ≈ 16.8 usable hours [estimate]); Haiku as overflow and outage fallback at an assumed 10% of traffic | ≈ $2 on the owner's Mac under a separate macOS user (electricity + overflow; F8 locally) | ≈ $195 with a dedicated box ($185 + ~$10) — over the cap | ≈ $285 ($185 + ~$90 overflow + F8) (2.5%) |
| **H. Hosted-open first:** F3 + F4 live on a ZDR route to Gemma 4 26B; recaps and Chronicle on Haiku Batch; F8 on Sonnet | ≈ $15 with F8 on Sonnet; ≈ $3 with F8 local or reused | ≈ $26.9 (2.3%) | ≈ $139 (1.2%) |
| **R. Recommended** | **≈ $2.9** (2.5%): H with F8 local or reused; headroom ≈ $2.9. Alternative: B on the owner's Mac, ≈ $2, testers only (§4.1) | **≈ $46** (4.0%): H, but the live crier and explainer on Haiku 4.5 (the demo's public face); headroom ≈ $12 | **≈ $139** (1.2%): H; headroom ≈ $441. Alternative: B with a dedicated box (≈ $285, 2.5%) if the owner wants no third-party processor |

Source of every number in this table: `model/revision_r1.py` sections C–E, except B (`cost_model.py`, v1).

**Recommendation:**
- **Use R at every size.** It fits the 5% cap at the upper-bound share everywhere. The Claude-first setup A does not fit at any size; using it would need the owner to raise the cap to ≈ 10% (1,000) or ≈ 8% (10,000) with a stated reason, and to ≈ 33% at 100 players, which is not sensible.
- **The share can be lower than the upper bound,** so the governor enforces the cap on the escrow actually accrued (G6). R stays within 5% down to these fractions of the upper bound: 50% (100 players), 79% (1,000) and 24% (10,000). Below that, the governor's first step moves the live crier from Haiku to the open model (R-lean: ≈ $27 at 1,000 players, within 5% down to 46% of the upper bound); after that, companion answers fall back to templates.
- **The companion free tier is off by default** until the playtest metric justifies it (§7.1); with it off, R costs ≈ $1.9 / $36 / $34.
- **If S3 shows the open model's Japanese is not good enough**, F3 uses Haiku 4.5 at ≤ 3 questions per player per season inside the cap (≈ $18 at 1,000 players, so R would be ≈ $54, 4.6%) or templates, and F4 live text stays on Haiku only where the cap allows.
- **Before M2** (no money): a fixed owner-set amount; the playtest needs ≈ $0–25 plus F7 if approved.
- **Hosted open models** are the cheapest operator option with no hardware ($13 a season for F3 + F4 live at 1,000 players, $124 at 10,000 [estimate]). The trade-off is a third party that sees the prompts: public data only under the fixed menu (G2), on a zero-data-retention route, listed as a processor in the privacy notice; and Japanese quality that is untested.

---

## 6. Fairness and verifiability

### 6.1 Shades stay deterministic [design; keeps DESIGN §7]

- **Decisions:** Shade actions come from committed code with committed parameters and seeds, using only public chain state and drand as inputs. The verifier replays every bell (DESIGN §7.3). No LLM decides.
- **Words (revision 1):** what a Shade says is chosen by the same policy from committed phrase banks with a seeded RNG, so the verifier replays the text itself (F5). No model writes a hidden Shade's words.
- **AI design without AI in consensus:** F8 lets "AI-designed Shades" be true without any model in consensus.

### 6.2 Reproducible open-model decisions: not proven, not needed for Season 1

**What it would take** [source: research §4]:
- pinned weight hashes and a pinned runtime build;
- one hardware architecture (A100 against H100 matched **0%**);
- batch size 1 or batch-invariant kernels (a 1.6–2.1× slowdown);
- greedy decoding.

**Why it does not suit Shades:**
- llama.cpp with several slots and MLX/Metal both diverge.
- Hosted APIs, Claude included, are not reproducible at all, and Claude 4.7 and later reject a non-default temperature.
- Verifying a Shade's LLM decisions would cost hundreds of times more than replaying code, would make the model consensus-critical, and would be harder to audit for fairness.

**Plan:** spike S4 only *measures* single-slot repeatability on the Mac, as research. No feature depends on it. Re-open the question (for Shade decisions or Shade wording) only if a cross-machine bit-exact result is shown on a pinned CPU or GPU image.

### 6.3 Player agents

- **Same rules and the same pipes:** same join (relay, and x402 in M2), same instructions, bucket, quotas, herald and seal rules. The MCP server is public and free, so nobody's agent has a better tool than anyone else's (P6).
- **Declaration is optional and cannot be enforced.** DESIGN §7.4 already says many humans run SDK bots. F6 rewards declaration with badges, not money. AI-written chat is flagged by the official kit (G9).
- **The economy already prices automation, with a thin margin.** An SDK-default bot returns 0.93–0.96×; the best-response criterion passes with only **~2 points** of margin (worst 0.980) [source: DESIGN §2.7, §5.4]. Cheap LLM agents make "always-online and skilled" nearly free to run.
- **Revision 1: the strong-agent run is an M2 money gate.** DESIGN §5.4 states the criterion as an M3 acceptance item; this plan proposes (owner decision, §8.3) to run it **before any money season opens**, with a "strong agent" profile: online every hour, **event-reactive** (wakes on public departures nearby, siege horns, clash reports) and playing at the very-skilled archetype's quality, at 1/2/5/10% shares, best response. The result is published on the join page either way. If it fails, money seasons wait, or the reactive actions are narrowed further.
- **Human parity (revision 1).** DESIGN goal 6 says a 15–30-minute daily session is enough, so an always-awake agent must not gain from reacting faster than a person can:
  - every player gets the same **alerts** the kit wakes on (web push; email only with consent): public departures within N provinces of their holdings (origin, arrival bell, mass), siege horns, clash reports on their holdings, the Shield ending;
  - every player gets **rule-based standing behaviours**: vigil hours and the auto-reinforce standing order exist in DESIGN (§6.3); a default-posture standing order is proposed for the conquest-rules work (a program change, to be decided there);
  - the official kit's event-reactive actions are limited to those same standing behaviours; anything else waits for its next scheduled decision. Third-party agents cannot be limited this way, which is why the criterion run measures the reactive worst case.
- **The operator's companion never knows more than the public tools.** The free tier sees only public data (G2), so it cannot give a paying-tier or operator-side information advantage.
- **Shades are never listed as agents** until the Reckoning, and are then shown in their own category.

### 6.4 What the operator must never do

- Run an LLM that plays in a money season, declared or not, unless every condition of §3.7 is met and the owner has decided so. An undeclared one would be a hidden operator wallet, which the design rules out.
- Feed keeper or relay data to any model, or run ai-gate where it can read keeper or key files (§4.1).
- Log, keep or train on players' free-text questions (G2 class 4).
- Let commentary choose its own focus (code picks, §3.4).
- Let any model write words for hidden identities (§3.5).

---

## 7. What to build when

| When | Build | Operator AI spend | Depends on |
|---|---|---|---|
| **Now → before M2** (in parallel with the conquest-rules planning) | **F1 v0**: summariser with the herald/RPC spot-check, tool layer (seal in code, journal), MCP server with the G10 warning, rule agent through MCP, LLM runner with Anthropic (explicit effort) and OpenAI-compatible adapters, usage log, USD caps, dedicated-session docs and key placement; on the Mode-A local stack (ports 41000–41999). **ai-gate** (generalised `advisor.mjs`) with the OS-isolation layout of §4.1. **F4 v0** template crier and clash explainer on `spectate.html` (demo from recorded fixtures and Mode-A replays). **F3 v0** BYO companion (localhost model or proxy; in-memory key) with the CSP. **Alerts** for the public events the kit wakes on. The spikes (§8) | ≤ $5 (spike) | Owner approvals (§8.3). No program change |
| **Private M1 playtest** (only if approved) | Declared agents among the testers (BYO); LLM crier (setup B, isolated user) with a with/without comparison; one showcase agent (F7, no money); **the demand metrics of §7.1** | Owner-set; ≈ $0–25, plus F7 ≈ $41 if run for a full season | Devnet approval (O-M1-18 currently not approved) |
| **M2 (money, lifecycle, Shade roster)** | **Strong-agent criterion run (gate before money)**, result on the join page; **F2** x402 join through the kit, with a USDC cap and human confirmation; fixed-menu companion free tier (F3), only if the metrics justify it; privacy notice with the processors per provider; **F8** AI-authored Shade parameters and phrase banks; verifier extension for "verified policy" replays (and for F7 only if the owner keeps it in money seasons); budget governor live on accrued escrow; model fallback chain; the move off the dated Haiku pin | Setup R, capped at ≤ 5% of the accrued operator share | M2 money, the M2 Shade roster |
| **M3 (society, public bot SDK)** | Public SDK and MCP release (docs, examples for Claude Desktop, Claude Code and an open model, with hardware terms); tools for the conquest rules, sieges, governance and chat, versioned by ruleset hash; **G9 `ai_written` flag** in the chat protocol and client, and the ToS rule; escaped rendering of all player text; **F6** registry, rationale commitments, leaderboard (if the metric justifies it); **F5** Shade voice as templates; prompt-injection defences and red-team corpus in CI; a re-run of the criterion on the final economy | As M2 | M3 chat and anchor; legal answer on hidden-AI chat and on player-run AI chat |
| **M4 (hardening, beta)** | Soak with agent fleets (including injection personas in chat); a red-team of the companion (BYO-key theft, XSS); the legal review covers AI disclosure for Shades and for player-run agents (Art. 50, provider policies); the cost model checked against beta usage (±30%) | Beta budget | M4 |
| **M5 (Season 1)** | Chronicle at the Reckoning; agents-vs-humans page after the Shade reveal | Setup R for the expected size | — |

### 7.1 Demand metrics before building the operator-paid parts (revision 1)

Measured in the private playtest (or, if none is approved, in Mode-A sessions with invited testers). Thresholds are [assumption] for the owner to set:

| Metric | Decides | Suggested threshold |
|---|---|---|
| Share of citizens with a declared agent (BYO kit or MCP) | Whether F6 (registry, leaderboard) is built in M3 | ≥ 5% of active citizens |
| Spectator retention (median minutes per visit, return visits) with vs without the LLM crier, same fixtures | Whether F4's LLM text is worth paying for over templates | ≥ 20% better with the crier |
| Companion questions per player per season (BYO and template modes) | Replaces the 30% × 3 × 20 = 18 guess; whether the operator-paid free tier is turned on | turned on if template/BYO use is ≥ 1 question per active player per day and players ask for a free tier |
| Kit installs and MCP connections, and how many agents run autonomously vs assisted | Whether the builders' pitch lands | tracked, no threshold |

---

## 8. Spike plan, hard spend cap and downloads

### 8.1 Spikes

| # | Spike | Spend | Downloads | Output and pass criteria |
|---|---|---|---|---|
| **S0** | Summariser + tool layer + MCP server + rule agent through MCP, on the recorded herald fixtures (`frontier-node/crates/agents/fixtures/herald-recorded/`) and a Mode-A local season at a slow scale (5×, so a bell is 120 s real: an LLM decision of ≈ 15 s fits) on ports 41000–41999; ai-gate with a mock provider, run as a separate OS user; the G2 leak test (grep every built prompt for plaintext, salt and key fields; check no request text reaches the log); the herald/RPC spot-check against a tampered herald; an injection corpus | $0 | none | Summary ≤ 3k tokens by character count (÷3 as a rough bound); the MCP rule agent plays one game day with 0 bad seals and 0 relay refusals by shape; the leak test finds nothing; the tampered herald is refused; ai-gate cannot read `.local/` or the keeper's files |
| **S1** | Exact token counts of every prompt (agent, companion, crier, explainer) for Haiku 4.5 and Sonnet 5.5 with the **free** `count_tokens` endpoint [source: "Token counting is free to use", subject to RPM limits] | $0 | none | The token shapes of §5.1 replaced by counts; the cost model re-run |
| **S2** | Claude runs on the local stack, **hard cap $4.50 in ai-gate, $5.00 outer** (prepaid credit or workspace limit). Run order, cheapest first, so the cap can only truncate the last item: crier 100 lines + 30 explanations on Haiku ≈ $0.45; companion 50 menu questions on Haiku + 10 on Sonnet ≈ $0.30; recaps 7 + 7 by Batch ≈ $0.15; Haiku agent 30 decisions ≈ $0.50–1.95; Sonnet 5.5 agent 15 decisions, half with up-front thinking off and half at the default ≈ $0.70–2.10 [estimate from §5.2]. **Total ≈ $2.1–5.0**; the run stops at the cap. Every call is logged to JSONL with `usage` | **≤ $5.00** | none | **Measured $ per decision with and without thinking** (sets §5.2's central value) and per line, within ±30% of §5 or the model is corrected; ≥ 95% valid tool calls; 100% of invalid actions refused by the tool layer; 0 seal or plaintext exposure to the model; crier lines pass the fact validator ≥ 95%; the owner reads 20 JA samples and rates them |
| **S3** | The same harness against a local open model through `llama-server`: Gemma 4 26B A4B first; Qwen3.6-35B-A3B or gpt-oss-20b only if disk allows. Measure prefill and generation tok/s (1 and 4 streams), latency per decision, valid tool-call rate, JA quality (20 samples, same prompts as S2), whether reasoning terminates, and the memory footprint (to fix the hardware terms of §0.1). Read (not use) the ZDR terms and prices of hosted routes for Gemma 4 26B | $0 API (electricity) | **Yes** (§8.2) | A go or no-go for "open model" in the kit, for setup R's open-model parts and for setup B; the measured speeds replace the [estimate] rows in research §2.2 |
| **S4** | Repeatability probe (research only): one prompt × 200 runs, single slot, greedy, then with a different thread count; hash the outputs | $0 | uses S3's model | Recorded agreement. No feature depends on it (§6.2) |
| **S5** | Red-team: injection strings in fake chat for the agent kit (local model or mocks), **including cross-tool exfiltration cases** (an MCP session with a mock filesystem/shell tool, told to read the session key file or `~/.config`); a malicious herald feeding false state to one wallet; adversarial menu inputs to the companion; XSS strings in player-text fields against the BYO-key page; check that the hard limits of the tool layer hold | $0 | none | No limit bypassed; no secret in any prompt; the mock exfiltration either fails or is documented as outside the kit's control with the G10 mitigation; the false herald state is refused; no script runs from player text |

**Order:** S0 → S1 → S2 and S3 in parallel → S4 and S5. About 2–3 engineer-weeks in total [estimate]. It does not touch the program or the keeper.

### 8.2 Downloads that need the owner's approval (sizes are estimates unless marked)

| Item | Size | Why | Note |
|---|---|---|---|
| llama.cpp (Homebrew bottle `llama.cpp`, or a source build of a pinned tag) | ≈ 10–50 MB [assumption] | OpenAI-compatible local server; the terminating GGUF path for Gemma 4 | MLX or LM Studio are alternatives (LM Studio ≈ 0.5 GB [assumption]) |
| Gemma 4 26B A4B instruct, GGUF Q4 (Hugging Face) | ≈ 16 GB [estimate, research] | The first candidate: Apache 2.0, MoE with 3.8B active | Fits the **35 GB free** |
| Qwen3.6-35B-A3B, GGUF Q4 | ≈ 20 GB [estimate] | Japanese and tool use; second candidate | Does **not** fit together with Gemma unless ≥ 20 GB more is freed |
| gpt-oss-20b (MXFP4) | ≈ 13–16 GB [source: "fits in about 16 GB"] | A cheap and strong tool caller | Instead of Qwen if disk is short |
| gpt-oss-120b (MXFP4) | ≈ 60–65 GB [source] | The strongest model that fits the Mac's memory | Needs ≥ 65 GB of free disk or an external drive |
| Gemma 4 E4B | ≈ 5 GB [estimate] | Small baseline for short lines and for the 16 GB-machine hardware claim | Optional |

**Disk:** 35 GB free on 2026-10-01 [source: `df`]. **The recommended first approval is llama.cpp plus Gemma 4 26B A4B only (≈ 16 GB).** Setting up the separate macOS user for S0/S3 is a system change the owner makes or approves.

### 8.3 Approvals the owner is asked for

1. **The Anthropic key** (`permutation-gateway/.local/anthropic-key`): S1 (free) and S2 (hard cap $5).
   - The old account ran out of credit on 2026-09-25 [source: research].
   - Topping up (≤ $5 suggested) is a purchase the owner makes.
   - A separate key or workspace with a spend limit is preferred [assumption: the limit exists for this account].
2. **The downloads in §8.2.** At least llama.cpp and Gemma 4 26B A4B; more only after freeing disk. And a separate macOS user for ai-gate and the model server (§4.1).
3. **Product decisions:**
   - (a) Shade chat: templates only (recommended; revision 1 drops the open-model option for hidden Shades). The legal question on hidden-AI chat **and on player-run AI chat** (EU AI Act Art. 50) goes to counsel now.
   - (b) Whether to offer an operator-paid companion free tier at all (fixed menu, off until the metrics of §7.1), and whether free text may ever be added to it.
   - (c) Showcase agents (F7): only in seasons with no money (recommended), or also in money seasons under all the conditions of §3.7.
   - (d) The AI spend rule: ≤ 5% of the **accrued** operator share in money seasons, plus a fixed cap before M2; setup R as the default. Using the Claude-first setup A needs an explicit cap raise.
   - (e) **Move the strong-agent bot-criterion run from M3 to the M2 money gate** (a change to DESIGN §5.4's timing), and add the alerts and standing behaviours of §6.3 for human parity (a default-posture standing order would be a program change for the conquest-rules work).
   - (f) An optional agent declaration registry and leaderboard (M3), if the metric justifies it.
   - (g) The `ai_written` chat flag (G9) as a protocol rule, and the matching terms-of-service rule.
   - (h) Move the advisor off the dated Haiku pin and its deflecting "are you an AI" line.
   - (i) The pitch wording of §0.2 (two tiers; "sealed scripted players"; hardware terms next to "local").
4. **Standing rules:** any devnet playtest with agents (O-M1-18, not approved today), any push, and any publication of the SDK or MCP package (npm) need separate approval.

---

## 9. Risks

| # | Risk | Likelihood / impact | Mitigation |
|---|---|---|---|
| K1 | **Hidden-AI chat conflicts with provider policy and AI law** (the Anthropic Usage Policy disclosure and no-impersonation rules; EU AI Act Art. 50 since 2026-08-02) | certain for Claude / high | Templates only for hidden Shades; legal review before M3 chat; disclose AI everywhere else; fix the old advisor prompt before any reuse |
| K2 | **Operator AI cost runs away** (companion use far above 30%; a crier per front at large scale) | medium / medium | The three budget layers (G6) on accrued escrow; the fixed menu and daily caps per citizen; template fallback; open models at scale; the governor picks the cache TTL |
| K3 | **Plaintext leak through operator services** (the keeper holds owners' in-bell plaintext) | low / high | ai-gate isolated at the OS level, herald-only; a CI leak test; the free companion never receives seal data |
| K4 | **Cheap strong agents erode the bot criterion** (a ~2-point margin; "always-online skilled" becomes nearly free) | medium / high | **Strong-agent, event-reactive run as an M2 money gate**; the published table; human parity (alerts and standing behaviours); the fixed Mandate menu and office limits already in force |
| K5 | **Prompt injection through chat (M3) makes players' agents misplay** | likely once chat exists / medium (to the player) | Untrusted-data wrapping in the kit; hard limits in the tool layer; human confirmation for money; a red-team corpus in CI |
| K6 | **Overclaiming in marketing** ("AI agents earn", "free local model", "Claude Desktop plays for you") | medium / high (trust, legal) | The §1 table is the claims list; the two-tier pitch; hardware terms next to "local"; the join page shows measured returns |
| K7 | **Model retirement or behaviour change** (the Haiku 4.5 pin; Sonnet 5.5's breaking changes: forced `tool_choice` and `temperature` return 400) | medium / low | Model ids in config; the fallback chain; the kit uses neither forced `tool_choice` nor `temperature` |
| K8 | **The self-host box**: the owner's Mac is a single point of failure, 35 GB of free disk leaves little room for logs, and open-model Japanese quality is unknown | high / low | Hosted fallback; S3 measures quality; log rotation with a quota; a ZDR hosted model or a dedicated box for public seasons |
| K9 | **Hallucinated commentary or advice misleads players** | medium / medium | Fact slots and the validator (G5); "AI suggestion" labels; code-picked coverage |
| K10 | **Herald load from agent polling** | low / medium | WS subscriptions, immutable per-bell files on a CDN, per-IP limits; the kit's default cadence |
| K11 | **Key theft from players** (agents on servers; BYO API keys in the browser exposed to XSS once player text is rendered in M3) | medium / medium (to the player) | Session keys only, with expiry, stored per G10; wallet key only for join and claim; a USDC cap and confirmation; BYO API key in memory by default or behind a localhost proxy; strict CSP; escaped rendering of player text; M4 red-team |
| K12 | **Rules churn** (the conquest rules come next; M3 governance) breaks agents | certain / low | Tools versioned by `ruleset_hash`; refusal on an unknown ruleset; the rule agent stays the reference |
| K13 | **Misdeclared agents on the leaderboard** | medium / low | Prestige only; a "verified policy" tier for proof; honest labels |
| K14 | **Public traffic on the owner's Mac reaches its secrets** (Anthropic key, deployer and oracle keys, wallets, keeper journals) | medium if setup B is used beyond testers / high | Separate macOS user (or VM/container) without read access to `.local/` and keeper files; authenticated tunnel for the inference port only; keeper on another host; no owner-Mac hosting for public seasons (§4.1) |
| K15 | **Cross-tool injection or a lying herald** (an injected chat message tells an MCP session to use its filesystem, shell, mail or wallet tools; a compromised herald feeds false state to chosen wallets) | medium (M3) / high (to the player) | Dedicated-session pattern and docs warning (G10); key file placement; RPC spot-check and outcome recomputation before acting (§3.1); S5 cases |
| K16 | **Players' intentions leak through companion questions** (targets typed before a seal exists; question text sent to a provider) | high if free text is allowed / high (fairness, privacy) | Fixed-menu free tier; free text only in BYO modes; if ever enabled, the G2 class-4 rules (no logs, retention 0, ZDR or self-host, banner, coordinate warning, privacy notice) |
| K17 | **Undisclosed AI chat by players' agents** (M3) | likely / medium (policy, legal, trust) | `ai_written` flag in the official kit with no off switch (G9); ToS rule; reports; legal review covers player-run agents |
| K18 | **Operator steering through AI text or an operator AI player** (model-phrased Shade lines; a showcase agent in a money season) | low if the rules hold / high (goal 5) | Template-only Shade voice that the verifier replays (§3.5); F7 only in no-money seasons, or under every condition of §3.7 |

---

## 10. Sources

**Read in this step:**
- Anthropic Usage Policy (effective 2025-09-15), read 2026-10-01 — https://www.anthropic.com/legal/aup
- Anthropic, Token counting ("free to use", rate limits; the ≈ 30% tokenizer note), read 2026-10-01 — https://platform.claude.com/docs/en/build-with-claude/token-counting
- EU AI Act Article 50 (applies from 2026-08-02 per Art. 113), read 2026-10-01 — https://artificialintelligenceact.eu/article/50/
- Apple Mac Studio store page (no prices shown; the M5 Max / Ultra line-up listed), read 2026-10-01 — https://www.apple.com/shop/buy-mac/mac-studio
- Revision 1: OpenRouter, Zero Data Retention guide (ZDR routing can be enforced globally or per request; it does not cover plugins), search result read 2026-10-01 — https://openrouter.ai/docs/guides/features/zdr
- Revision 1: Bankless, "Eliza Daydreams: A New Blueprint for Onchain Gaming Agents" and games.gg, "Daydreams: A New Approach to Web3 Gaming with Eliza AI Agents" (Eternum planned 1,000+ LLM agent players; 2025), search results read 2026-10-01 — https://www.bankless.com/read/eliza-daydreams-onchain-gaming , https://games.gg/news/daydreams-gaming-eliza-ai-agents/

**From the research notes** (`research/models-prices.md` S1–S33; `research/old-game-inventory.md`): Claude prices and caching minimums, thinking settings, deprecations, Gemma 4 model card and licences, OpenRouter prices, Mac speed measurements, determinism papers, the W3 agent shape and the old game's per-tick estimates.

**Repo, read-only:**
- `docs/frontier/DESIGN.md` §1 (goals 5 and 6), §2, §5.4 (criterion, worst 0.980, "acceptance criterion for M3"), §6.2 (Depart: the arrival bell is public, the direction is not), §6.4 (threat table: "Operator sees sealed orders"), §6.3 (sieges, vigil hours, the auto-reinforce standing order), §7, §9, §11.3, §12, §13;
- `docs/frontier/SUMMARY.ja.md` §8–§11;
- `docs/frontier/m1/M1-CONTRACT.md` §0, §5.6, §8.3–§8.6 (`/h/clash` fields incl. `outcomeDigest`);
- `docs/frontier/DECISIONS.md` part A;
- `permutation-gateway/src/advisor.mjs` (model pin and system prompt);
- `permutation-gateway/agents/llm-agent.mjs` (header);
- `frontier-node/crates/agents/src/{lib,policy}.rs` (module list and the `Intent` enum);
- `permutation-server/web/frontier/spectate.html` (header).

**Machine:** `df -h` on 2026-10-01 (926 GiB volume, 35 GiB free).

**Cost model:** `model/cost_model.py` and `model/revision_r1.py` (this folder).

---

## 11. Revision notes (revision 1, 2026-10-01)

Each review issue was checked against the plan, the cost model, the research notes and DESIGN. Verdicts: **accepted** (the issue holds and the plan is changed), **partly accepted** (the core holds; a stated detail does not, and the rebuttal is given).

| # | Issue (short) | Verdict | Check | Change |
|---|---|---|---|---|
| 1 | §0.3 headline gives setup A totals as "the recommended setup"; share range wrong | **Accepted** | $25/$135/$940 are A's totals; the v1 recommendation was B at 100 (≈ $2), A at 1,000, B box at 10,000 (≈ $285). 135/1,160 = 11.6%, 940/11,600 = 8.1%, 25/116 = 21.6% [estimate] | §0.3 now gives the new recommended setup R per size (≈ $3 / $46 / $139; 2.5% / 4.0% / 1.2%) and labels A separately with its correct shares (recomputed without F7: $38 / $115 / $875; 33% / 9.9% / 7.5%) |
| 2 | The recommended setup breaks the plan's own 5% cap; the share is an upper bound | **Accepted** | 5% of $1,160 = $58 < $135; F3 alone ($79) > $58; DESIGN §8.8 gives the share at day-0 prices and 30% staking | New §5.4 with a cap row, headroom and setup R that fits at every size; the cap is enforced on accrued escrow with a fixed step-down order (G6); companion free tier fixed-menu, on a ZDR open model, off by default; the realised-share break-even fractions are shown |
| 3 | Sonnet agent cost is a low case shown as central | **Partly accepted** | Recomputed: W3-like 5 × 600 → $0.075/decision, ≈ $50 hourly; kit cap 6 × 400 → ≈ $50; 6 × 1,500 → ≈ $95 [estimate, `revision_r1.py` A]. **Rebutted detail:** "the old game counted ≈ 25–37k tokens per tick against the plan's ≈ 24k" compares units wrongly: the plan's 24k is before the Sonnet tokenizer; in Sonnet tokens the base shape is 31.2k, inside 25–37k. The light part of the shape is the output and the number of calls, not the input | §0.4c and §5.2 now give ranges ($30–95 hourly, $5–16 at 4 a day on Sonnet; $1.85–7 at 4 a day on Haiku); the kit sets effort/thinking explicitly; S2 measures with and without thinking and sets the central value; F7 cost shows its high end |
| 4 | "20–50× cheaper per token" overstated | **Accepted** | Ratios: Gemma 4 26B 13× / 20×, gpt-oss-120b 27× / 29×, gpt-oss-20b ≈ 56× [source prices, estimate]; on cached workloads 735/105 = 7.0×, 128/20 = 6.4×. The "20–50×" came from the research summary | §5.1 note and §5.4 text corrected; gpt-oss prices added to §5.1 |
| 5 | F7 contradicts G1 and DESIGN goal 5 | **Accepted** | G1 said "no operator-run model decides any game action"; F7 does; goal 5: "Nobody, the operator included, can steer the result" [source]. v1 also counted F7 in the 1,000/10,000 columns while saying "only in demo seasons" | F7 limited to no-money seasons; G1 carries the explicit exception; the conditions for money seasons are listed as an owner option; F7 removed from all money-season totals; K18 added |
| 6 | Free-text companion leaks players' intentions; G2 omits it | **Accepted** | A typed target precedes the seal (arrival ≤ departure + 72 bells [source: DESIGN §6.2]); DESIGN §6.4 lists "Operator sees sealed orders: no operator deposit path" [source]; v1's "never learns which target was chosen" fails once a player types it; hosted models would see the text | G2 class 4 (player free text) with its rules; free tier fixed-menu, no free text by default; planning questions in BYO; the "never learns" sentence corrected; privacy notice and processors in M2; K16 added |
| 7 | Strong-agent fairness check too late; wake-up trigger not public | **Accepted** | v1 put the strong-agent run in M3, following DESIGN §5.4's "acceptance criterion for M3", while F2 ships with money in M2. Depart makes the arrival bell and departure mass public but not the direction [source: DESIGN §6.2], so "a hostile departure towards an own March" cannot be known | The run moves to the M2 money gate (an owner decision that changes DESIGN's timing); profile hourly + event-reactive + very-skilled; human parity via alerts and standing behaviours; the kit's reactive power limited to those; the wake-up trigger corrected to origin, arrival bell, mass, within N provinces |
| 8 | Model-phrased hidden Shade lines are a covert steering channel | **Accepted** | An intent tag replays; free wording does not, and a text hash proves nothing about authorship | Templates are the only voice for hidden Shades; the verifier replays the text from committed phrase banks and a seeded choice; open-model phrasing only for openly disclosed AI characters; the v1 option 2 is forbidden until S4-style reproducibility is proven |
| 9 | The recommendation and the cap disagree at every size; F7 costed inconsistently | **Accepted** (overlaps issues 1, 2, 5) | $22 = two Haiku agents, $61 = two Sonnet agents, while the text said one Sonnet + one Haiku = $41.4 [estimate, cost_model.py] | One consistent F7 figure ($41) outside money totals; recommendation and cap agree (setup R); headroom column added; A shown as needing an explicit cap raise |
| 10 | Undeclared player LLM agents chatting with humans in M3 | **Accepted** (with a limit) | F1 + M3 chat lets players run undisclosed AI chat through the official kit; F6 declaration is optional. Limit: the operator can flag only what its own kit sends; third-party kits can be ruled on but not forced | G9 `ai_written` flag with no off switch in the official kit and client; ToS rule; the legal review covers player-run agents; K17 added |
| 11 | Prompt injection across tools in MCP clients; herald as an unchecked input | **Accepted** | The kit's limits cover only its own tools; the herald is "never a trust root" [source: §4 diagram, DESIGN] yet fed the whole view | G10 dedicated-session pattern, docs warning, key file placement; herald/RPC spot-check and outcome recomputation before any write; S5 cross-tool and false-herald cases; K15 added |
| 12 | BYO keys in browser storage exposed to XSS | **Accepted** | v1 stored the key in local storage with a warning; M3 renders player text | Key in memory by default; localhost proxy recommended; storage only by opt-in; strict CSP; escaped rendering from M3; K11 and the M4 red-team extended |
| 13 | Setup B serves public traffic from the owner's Mac | **Accepted** | The Mac holds the key file, deployer/oracle keys, wallets and playtest keeper/relay [source: the owner's environment notes and the task]; v1's K8 listed only "single point of failure"; 35 GB free [source: `df`] | §4.1 placement and isolation (separate macOS user, tunnel, keeper elsewhere, log quota; owner's Mac for own testers only); G2 says isolation is at the OS level; K14 added; B kept only as the testers-only alternative at 100 players |
| 14 | The selling point is overstated and narrow | **Partly accepted** | (1) True for Claude Desktop, which acts only while the user chats; **detail:** a headless MCP client the user schedules (e.g. Claude Code run non-interactively) can play autonomously, so the plan says "the runner or a headless client you schedule" rather than "only the API-key SDK". (2) Hardware estimate accepted (~32 GB+ Mac or 24 GB GPU for 26B A4B [estimate]). (3) Confirmed: Eternum has marketed agent play since 2025 [source, §10]. (4) Accepted. (5) Accepted | §0.2 two-tier pitch; assisted vs autonomous stated in §1 and §3.1; hardware terms next to "local"; "sealed scripted players"; the real differentiators named; §7.1 demand metrics gate F3's free tier and F6; K6 widened |

**Unchanged on purpose:** the self-host box price and speed assumptions (still unmeasured; S3 replaces them); the F3 usage upper case (18 questions per player) as the cost ceiling until the playtest metric replaces it; the spend cap of the spikes ($5).
