# Research: models and prices for AI agents in Wylls

- **Date of research:** 2026-10-01. Every price below was read on that date unless stated otherwise.
- **Scope:** Claude models (prices, caching, batch, tool use), open-weight models that could run on the owner's Mac (Apple M4 Max, 16-core CPU, 128 GB) or on a rented GPU, their licences, tool calling and speed, and whether open-model inference can be made bit-reproducible so a verifier can re-run an AI's decisions.
- **Method:** public web pages only (Anthropic docs, Google model cards, Hugging Face public API, OpenRouter public model list, papers, benchmark threads). No paid API calls, no downloads of models or tools, no chain transactions. The owner's API key was not touched.
- **Labels:** `[source]` = read from a cited page (the source list is at the end, keyed S1, S2, ...). `[estimate]` = my calculation or extrapolation from sourced numbers. `[assumption]` = a value I picked to make a calculation possible, to be replaced by measurement.

---

## 0. Answers in short

1. **Gemma 4 exists.** Google released it on 2026-03-31 in E2B, E4B, 26B A4B (MoE) and 31B (dense) sizes, then a 12B "Unified" model on 2026-06-03, all under **Apache 2.0** [source S5, S6, S20]. Gemma 3 (2025) is under the older Gemma Terms of Use, which carry a prohibited-use policy that must be passed down to users, and Google can restrict use remotely [source S7, S20].
2. **Any of the recent Apache 2.0 models fits on the Mac.** Gemma 4 26B A4B, Gemma 4 31B, Qwen3.6-35B-A3B, gpt-oss-20b, gpt-oss-120b and Mistral Small 4 (119B, 6B active) all fit in 128 GB at 4-bit. The MoE models with 3–6B active parameters should generate roughly 50–110 tokens/s for one stream [estimate from S12, S13, S14]. That is enough for chat flavour, commentary and simple strategy for tens of agents. It is not a server for thousands of players.
3. **For low cost and real-time use, Claude Haiku 4.5 is still the cheapest Claude model** at $1 / $5 per million input/output tokens [source S1]. Claude Sonnet 5.5 (released 2026-09-28) costs $2 / $10, the same as Sonnet 5 [source S1, S3]. Hosted open models cost 20–50 times less per token: on OpenRouter, gpt-oss-120b is $0.037 / $0.17 and Gemma 4 26B A4B is $0.0765 / $0.255 [source S9]. The prices of the two current Claude models in the repo are unchanged, but `src/advisor.mjs` pins `claude-haiku-4-5-20251001`. Its retirement date is listed as "not sooner than 2026-10-15" and no deprecation notice has been posted [source S2]. Anthropic gives at least 60 days' notice [source S2], so the earliest it can retire is about 2026-11-30 [estimate].
4. **Bit-reproducible LLM decisions are possible, but only on a pinned stack.** That means the same weights hash, the same runtime build, the same hardware architecture, batch size 1 (or batch-invariant kernels) and greedy decoding. Different GPU architectures do not match: EigenAI measured 100% match on the same architecture and **0%** between A100 and H100 [source S16]. Hosted APIs, Claude included, are not reproducible. Claude 4.7 and later models reject non-default `temperature`, so greedy decoding is not even an option there [source S2]. DESIGN.md §7 already says no LLM decides any action. That rule is the cheapest way to stay verifiable, and this research supports keeping it (see §5).

---

## 1. Claude models (Anthropic API), prices as of 2026-10-01

### 1.1 Price table (USD per million tokens) [source S1]

| Model (API id) | Input | 5-min cache write | 1-h cache write | Cache read | Output | Batch in / out | Context | Max output |
|---|---|---|---|---|---|---|---|---|
| Claude Haiku 4.5 (`claude-haiku-4-5-20251001`) | 1.00 | 1.25 | 2.00 | 0.10 | 5.00 | 0.50 / 2.50 | 200K | 64K |
| Claude Sonnet 5.5 (`claude-sonnet-5-5`), released 2026-09-28 | 2.00 | 2.50 | 4.00 | 0.20 | 10.00 | 1.00 / 5.00 | 1M | 128K |
| Claude Sonnet 5 (`claude-sonnet-5`), used by `agents/llm-agent.mjs` | 2.00 | 2.50 | 4.00 | 0.20 | 10.00 | 1.00 / 5.00 | 1M | 128K |
| Claude Sonnet 4.6 | 3.00 | 3.75 | 6.00 | 0.30 | 15.00 | 1.50 / 7.50 | 1M | 128K |
| Claude Opus 5.5 (`claude-opus-5-5`) | 4.00 | 5.00 | 8.00 | 0.20 (0.05×) | 20.00 | 2.00 / 10.00 | 1M | 128K |
| Claude Opus 5 | 5.00 | 6.25 | 10.00 | 0.50 | 25.00 | 2.50 / 12.50 | 1M | 128K |
| Claude Fable 5.1 (top tier) | 10.00 | 12.50 | 20.00 | 0.25 (0.025×) | 50.00 | 5.00 / 25.00 | 1M | 128K |

Notes, all [source S1, S3, S4] unless marked:

- **Sonnet 5 price:** $2 / $10 was introductory pricing through 2026-08-31. Anthropic has now made it the standard price, and the planned rise to $3 / $15 will not happen.
- **Batch API:** 50% off input and output. It stacks with caching. It is asynchronous, so use it for recaps, digests and season summaries, not live chat.
- **Prompt caching:** a cache read costs 0.1× the base input price (0.05× on Opus 5.5, 0.025× on Fable 5.1). A 5-minute write costs 1.25×, a 1-hour write 2×. The **minimum cacheable prefix** is 512 tokens on Sonnet 5.5 and Opus 5/5.5 but **4,096 tokens on Haiku 4.5** [source S4]. The current advisor prompt in `src/advisor.mjs` is about 150–250 tokens [estimate], so it can never be cached on any model, and caching does not lower its cost.
- **Tool use overhead:** using tools adds a hidden system prompt of 286 tokens (Opus 5.5, Sonnet 5.5), 354 (Sonnet 5) or 496 (Haiku 4.5), plus the tool schemas themselves.
- **Tokenizer:** Claude 4.7 and later models (Sonnet 5/5.5 and Opus 5/5.5 included) use a newer tokenizer that produces about 30% more tokens for the same text. Haiku 4.5 uses the old one. So for the same text, Sonnet 5.5 costs about 2 × 1.3 ≈ **2.6× Haiku 4.5**, not 2× [estimate].
- **Thinking:**
  - Sonnet 5.5 uses adaptive thinking by default. Its lowest setting, `between_tools`, turns off up-front thinking.
  - Opus 5.5 cannot disable thinking; its default effort is `medium`.
  - Haiku 4.5 does not think unless asked.
  - Thinking tokens are billed as output, so for short chat lines Haiku 4.5, or Sonnet 5.5 with up-front thinking off, keeps output cost predictable.
- **Breaking changes in Sonnet 5.5 vs Sonnet 5** that matter to `llm-agent.mjs`:
  - forced `tool_choice` (`any` / `tool`) returns a 400;
  - text between tool calls now comes back in `thinking` blocks;
  - thinking blocks are tied to the model and conversation;
  - non-default `temperature` / `top_p` / `top_k` return a 400.

  `llm-agent.mjs` does not set `tool_choice` or `temperature` (checked by grep, read-only), so the switch looks low-risk. It still needs a test run.
- **Other modifiers:** `inference_geo: "us"` multiplies every price by 1.1. Fast mode exists for Opus only ($8 / $40 on Opus 5.5). Web search is $10 per 1,000 searches.
- **Rate limits and free credits:** not researched here. Limits depend on the account's tier.

### 1.2 Which Claude tier for which game job [estimate]

| Job | Suggested model | Why |
|---|---|---|
| One-line chat flavour (operator AI members, Shade banter) | Haiku 4.5, or Sonnet 5.5 with up-front thinking off | Cheapest and fastest; no thinking tokens. Haiku's retirement date must be watched. |
| Herald / commentary digest per bell or per day | Haiku 4.5 or Sonnet 5.5 via the **Batch API** when it need not be live | Batch halves the price; quality matters more for public text. |
| Reference "Claude plays the game" agent (the runner's own key) | Sonnet 5.5 (same price as Sonnet 5), effort `low`/`medium` | Good tool use; prompt caching works from 512 tokens; the runner pays. |
| Showcase or tournament agent | Opus 5.5 | 2× Sonnet per token, but cache reads cost the same ($0.20). |

---

## 2. Open-weight models (as of 2026-10-01)

### 2.1 Candidates and licences

Licences are taken from the Hugging Face public API `cardData.license` [source S8] unless marked. "HF repo created" is the date the repository was created, not necessarily the public release date.

| Model | Params (total / active) | Context | Licence | Tool calling | Notes |
|---|---|---|---|---|---|
| **Gemma 4 E2B / E4B** | 5.1B / 2.3B effective; 8B / 4.5B effective | 128K | Apache 2.0 [S5, S8] | Native structured tool use [S5] | Audio input; aimed at phones/edge. Tau2 24.5% / 42.2% [S5]: too weak for strategy, fine for short lines. |
| **Gemma 4 12B Unified** (2026-06-03) | 11.95B dense | 256K | Apache 2.0 [S5, S8] | Native [S5] | MMLU-Pro 77.2%, Tau2 69.0% [S5]. |
| **Gemma 4 26B A4B** (2026-03-31) | 25.2B / 3.8B (MoE, 128 experts, 8 active) | 256K | Apache 2.0 [S5, S8] | Native [S5] | MMLU-Pro 82.6%, LiveCodeBench v6 77.1%, Tau2 68.2% [S5]. Known MLX issue: with reasoning on, it may never stop thinking under LM Studio's MLX engine; the GGUF/llama.cpp path terminates [S15]. |
| **Gemma 4 31B** (2026-03-31) | 30.7B dense | 256K | Apache 2.0 [S5, S8] | Native [S5] | MMLU-Pro 85.2%, LiveCodeBench v6 80.0%, Tau2 76.9% [S5]: the strongest Gemma. |
| Gemma 3 27B (2025-03) | 27B dense | 128K | **Gemma Terms of Use** [S7, S8] | Prompt-based, no dedicated format [assumption] | Commercial use allowed, but the prohibited-use policy must flow down to users, a notice file is required, and Google "reserves the right to restrict (remotely or otherwise) usage" [S7]. Superseded by Gemma 4 for this project. |
| **gpt-oss-20b** (2025-08) | 21B / ~3.6B (MoE, MXFP4) | 128K | Apache 2.0 [S8, S10] | Yes, trained for tools (harmony format) [S10] | Fits in about 16 GB [S10]. Mostly English-centred [assumption]; test Japanese. |
| **gpt-oss-120b** (2025-08) | 117B / ~5.1B (MoE, MXFP4) | 128K | Apache 2.0 [S8, S10] | Yes [S10] | Native 4-bit; about 60–65 GB, so it fits in 128 GB [S10]. |
| **Qwen3.6-35B-A3B** (2026-04-16) | 36B / 3B (MoE) | 256K | Apache 2.0 [S8, S11] | Yes; thinking and non-thinking modes [S11] | Strong agentic coding for its size [S11]; many languages including Japanese [assumption from the family's 201-language claim, S21]. |
| Qwen3.6-27B / Qwen3.8-27B (HF repo created 2026-08-05) | 27.8B dense | 256K+ | Apache 2.0 [S8] | Yes [S9: OpenRouter lists `tools`] | Newest dense Qwen in the 27B class. |
| Qwen3.5-122B-A10B (2026-02) | 125B / 10B (MoE) | 256K | Apache 2.0 [S8] | Yes; BFCL-V4 72.2 claimed [S21, secondary] | Fits at 4-bit (about 65–70 GB) [estimate]. |
| **Mistral Small 4** (`Mistral-Small-4-119B-2603`, 2026-03-16) | 119B / 6B (MoE) | 256K | Apache 2.0 [S8, S22] | Yes (merges Devstral agentic coding) [S22, secondary] | Fits at 4-bit (about 65 GB) [estimate]. |
| Llama 4 Scout (2025-04) | 109B / 17B (MoE) | up to 10M | **Llama 4 Community Licence** [S8, S23] | Yes | 700M MAU cap, a "Built with Llama" display requirement, and derivatives must be named "Llama…". The acceptable-use policy withholds rights to the **multimodal** models from people and companies in the **EU** [S23, S24]. Meta has not released an open model since and is reported to be moving to closed models [S25, secondary]. **Not recommended.** |
| FunctionGemma 270M (2025-12-18) | 0.27B | n/a | Gemma licence family [S6] | Built only for function calling [S6] | Could be a tiny intent parser; not a strategist. |

**Recommendation for this project [estimate]:**

- **Gemma 4 (26B A4B or 31B) and Qwen3.6-35B-A3B** are the best fit. Both are Apache 2.0 with no MAU cap or flow-down rules, have native tool use, handle many languages (the game ships Japanese), and are cheap to host.
- **gpt-oss-120b** is the strongest single model that fits the Mac, but its Japanese should be checked first.
- **Avoid Llama 4 and Gemma 3** because of their licence terms.

### 2.2 Speed on the owner's Mac (M4 Max, 128 GB)

The 128 GB M4 Max is the 16-core CPU / 40-core GPU part with **546 GB/s** memory bandwidth [source S12, which lists that chip at 546 GB/s; that 128 GB implies this variant is an assumption from Apple's line-up]. Token generation is bound by memory bandwidth, so speed tracks the bytes of *active* weights read per token.

Measured numbers on an M4 Max:

| Model | Runtime | Generation tok/s (one stream) | Source |
|---|---|---|---|
| LLaMA 7B Q4_0 | llama.cpp | 83 (prompt processing 886) | S12 |
| LLaMA 7B Q8_0 / F16 | llama.cpp | 54 / 32 | S12 |
| Qwen3-30B-A3B (4-bit) | mlx-lm / llama.cpp / vllm-mlx | 107 / 90 / 110 | S13 (M4 Max 128 GB) |
| Nemotron-30B-A3B | mlx-lm / llama.cpp | 102 / 85 | S13 |
| Gemma 3 4B | mlx-lm / llama.cpp | 105 / 123 | S13 |
| gpt-oss-20b | (as tested) | 81 at concurrency 1; 20 per stream at concurrency 8 | S14 (M4 Max **64 GB**) |
| Gemma 3 27B | (as tested) | 21.6 at concurrency 1 | S14 |
| 30B-class dense, Q4 | MLX | 30–40 (band) | S17 (aggregated, secondary) |
| 70B-class dense, Q4 | MLX | 18–24 (band) | S17 |

Throughput can be raised by batching several requests: S13 measured a Qwen3-0.6B aggregate rising from 441 to 1,642 tok/s at 16 concurrent requests, with smaller gains for larger models.

Estimates for the candidate models (no local runtime is installed, so none of these was measured):

| Model (4-bit unless noted) | Approx. memory | Generation tok/s, one stream | Basis |
|---|---|---|---|
| Gemma 4 E4B | ~5 GB | 100–150 | [estimate] from Gemma 3 4B at 105–152 (S13) |
| Gemma 4 26B A4B | ~16 GB | 70–100 | [estimate] from Qwen3-30B-A3B at 90–110 (S13); 3.8B active |
| Qwen3.6-35B-A3B | ~20 GB | 80–110 | [estimate] same class as Qwen3-30B-A3B (S13) |
| Gemma 4 31B dense | ~18 GB | 18–25 | [estimate] from Gemma 3 27B at 21.6 (S14) and the 30B band (S17) |
| gpt-oss-120b (MXFP4) | ~63 GB | 40–60 | [estimate]: M2 Ultra (800 GB/s) measured 79.7 tok/s (S18); scaled by 546/800. One aggregate page claims only 10–14 tok/s on M4 Max (S17); treat as unconfirmed. |
| Mistral Small 4 (119B / 6B active) | ~65 GB | 35–55 | [estimate] by active size vs gpt-oss-120b |
| Qwen3.5-122B-A10B | ~68 GB | 25–40 | [estimate] by active size |

- **Prompt processing (prefill)** also matters for agents that read a large state summary. A 7B Q4 model processes about 880 tok/s [S12], so an 8K-token prompt takes about 9 s on a 7B. A 3–4B-active MoE should be similar or faster [estimate]. Prefix caching in llama.cpp or MLX avoids paying that again for the stable part of the prompt [assumption].
- **Electricity:** the Mac draws about 60–90 W under GPU load [assumption]. At $0.20/kWh [assumption], an hour costs about $0.016. At 80 tok/s that hour produces about 290K output tokens, roughly **$0.06 per million output tokens** [estimate]. That is far below any API, but it is one machine with a single point of failure, and it is the owner's laptop/desktop.

### 2.3 Hosted open-model prices (if the owner does not want to self-host)

These are the lowest listed prices on OpenRouter's public model list, read 2026-10-01 [source S9], in USD per million tokens. Prices vary by provider and change often. `:free` variants exist for several models but are rate-limited and unsuitable for production [assumption].

| Model | Input | Output | Context | Tools |
|---|---|---|---|---|
| openai/gpt-oss-20b | 0.018 | 0.09 | 131K | yes |
| openai/gpt-oss-120b | 0.037 | 0.17 | 131K | yes |
| google/gemma-4-26b-a4b-it | 0.0765 | 0.255 | 262K | yes |
| google/gemma-4-31b-it | 0.09 | 0.34 | 262K | yes |
| google/gemma-3-27b-it | 0.08 | 0.45 | 131K | yes |
| qwen/qwen3.6-35b-a3b | 0.15 | 1.00 | 262K | yes |
| qwen/qwen3.8-27b | 0.42 | 3.00 | 1M | yes |
| mistralai/mistral-small-2603 (Small 4) | 0.15 | 0.60 | 262K | yes |
| meta-llama/llama-4-scout | 0.10 | 0.30 | 1.3M | yes |
| anthropic/claude-haiku-4.5 (for comparison) | 1.00 | 5.00 | 200K | yes |
| anthropic/claude-sonnet-5.5 (for comparison) | 2.00 | 10.00 | 1M | yes |

### 2.4 Rented GPU

The H100 80 GB on-demand median was **$3.25 per GPU-hour** in September 2026. The range ran from about $1.49 to $13: RunPod $2.69–3.49, Lambda $3.29–3.99 [source S19, secondary aggregators].

One H100 holds gpt-oss-120b in MXFP4 [source S10]. At an assumed 2,000 tok/s batched aggregate [assumption] and full use, $3.25/h comes to about $0.45 per million tokens [estimate], which is *more* than OpenRouter's gpt-oss-120b price. A rented GPU therefore only pays off for:

- (a) a **pinned, reproducible verifier environment** (§4), or
- (b) data-control needs.

It does not pay off on price.

---

## 3. Worked cost per game job (all [estimate]; token counts are [assumption])

### W1. One chat-flavour line

Assumed size: 300 input tokens (system plus incoming message), 60 output tokens.

| Model | Cost per line | Per 100,000 lines |
|---|---|---|
| Haiku 4.5 | $0.00060 | $60 |
| Sonnet 5.5, up-front thinking off (×1.3 tokenizer) | $0.00156 | $156 |
| Opus 5.5, +100 thinking tokens assumed (×1.3 tokenizer) | $0.0051 | $510 |
| Gemma 4 26B A4B (OpenRouter) | $0.000038 | $3.8 |
| gpt-oss-120b (OpenRouter) | $0.000021 | $2.1 |
| Local Mac, Gemma 4 26B A4B (electricity only; about 1.1 s per line at about 80 W) | ~$0.000005 | ~$0.5 |

The local Mac can make about 3,000 lines per hour in one stream, more if batched.

### W2. Herald/commentary digest

Assumed size: 4,000 input tokens, 400 output tokens.

| Model | Live | Batch |
|---|---|---|
| Haiku 4.5 | $0.0060 | $0.0030 |
| Sonnet 5.5 (×1.3 tokenizer, thinking off) | $0.0156 | $0.0078 |
| Opus 5.5 (+500 thinking tokens assumed) | $0.041 | $0.021 |
| Gemma 4 31B (OpenRouter) | $0.00050 | — |
| gpt-oss-120b (OpenRouter) | $0.00022 | — |

### W3. Reference agent decision

Assumed shape: a tool loop of 5 model calls. Each call has a 10K-token cached prefix (tools, system, history), 1,500 new input tokens written to cache, and 600 output tokens (300 text + 300 thinking).

| Model | Per call | Per decision | Per agent-day at 48 decisions (every 30 min [assumption]) |
|---|---|---|---|
| Sonnet 5.5 | 0.002 + 0.00375 + 0.006 = $0.0118 | $0.059 | $2.8 |
| Haiku 4.5 (300 output, no thinking) | $0.0044 | $0.022 | $1.06 |
| Opus 5.5 | $0.0215 | $0.11 | $5.2 |
| gpt-oss-120b (OpenRouter, no cache discount assumed) | $0.00053 | $0.0026 | $0.13 |

The existing `llm-agent.mjs` default budget of 2,000,000 input tokens per run caps the worst case at about $4 of uncached Sonnet input, plus output [estimate].

**Reading:**

- **Operator-paid flavour text is cheap at any realistic scale.** With 250 Shades at 50k players (DESIGN §7.1) each sending 20 lines a day, that is 5,000 lines/day: Haiku 4.5 costs about $3/day and a hosted Gemma 4 about $0.20/day [estimate].
- **Operator-paid *playing* agents are the cost risk.** With Sonnet, 250 Shades × $2.8/day comes to about **$700/day**. With hosted gpt-oss-120b it is about $33/day; on the local Mac it is near zero, but throughput-limited [estimate].
- This supports the existing rule: Shades decide with committed deterministic code, LLMs only phrase. The paid, AI-plays-the-game pitch belongs to **player-run agents on the player's own key or own local model** (the public SDK/MCP of M3).

---

## 4. Can open-model inference be made bit-reproducible for a verifier?

### 4.1 What breaks determinism

**Floating-point non-associativity combined with batch variance.** Kernels choose different reduction orders depending on batch size and shape, and server load changes the batch size.

- Thinking Machines ran Qwen3-235B 1,000 times at temperature 0 and got **80 unique completions**, with the first divergence at token 103. With batch-invariant kernels (RMSNorm, matmul, attention), all 1,000 were identical. On Qwen3-8B the cost was a **1.6–2.1× slowdown** [source S26].
- vLLM now has a flag for this, `VLLM_BATCH_INVARIANT=1`. It is in beta, needs NVIDIA compute capability 8.0 or higher (or Intel XPU), and the docs do not promise identical results across GPU types or versions [source S27]. One independent measurement found a 54–67% throughput loss in the default configuration [source S28, secondary].

**llama.cpp server with several slots.** Here 8 parallel slots gave 5–8 unique completions for the same prompt at temperature 0. A single slot gave the same text every time, but logits still varied slightly; this was seen on an M1 Mac and on H100/A100 [source S29].

**MLX on Apple Silicon.** Metal kernels are not guaranteed bit-reproducible across runs or devices. One open issue reports a deterministic but large logit divergence between a Mac and an iPhone on identical inputs, with about 18% of answers flipped [source S30]. A blog reports run-to-run differences on MLX/Metal, where CPU/NumPy matched exactly [source S31, secondary, some of its numbers look implausible].

**Across hardware architectures.** EigenAI used custom deterministic GEMM kernels, pinned CUDA and driver versions, fixed reduction order and a fixed-seed PRNG on llama.cpp. They got **100% match over 10,000 runs** on the same host and on different hosts of the same architecture, with about **1.8%** latency overhead. A100 against H100 matched **0%**, so verifiers must use the same GPU family [source S16]. A 2026-08 paper got CUTLASS and Triton kernels to agree bitwise at every linear layer by rounding INT8 weight scales to powers of two. The cost was −0.28% to +0.71% perplexity. It also warns that tolerance-based conformance tests are blind to whole classes of faults [source S32].

**Fixed-point VM.** opML runs a 7B LLaMA in a deterministic VM using fixed-point and soft-float. A dispute is bisected down to a single instruction and settled on-chain; it runs on a CPU, slowly [source S33].

**Hosted APIs (Claude, OpenRouter providers).** These are not reproducible by design: batching on the provider side, unknown hardware, silent model updates on aliases [assumption, consistent with S26]. Claude 4.7 and later reject non-default `temperature`, so even greedy decoding cannot be requested [source S2].

### 4.2 What a reproducible "AI decision" pipeline would need [estimate]

1. **Commit to the weights.** Put the SHA-256 of the exact weight file (for example one GGUF file) in the Season at genesis, next to `policy_code_hash`.
2. **Commit to the runtime.** Pin a container digest with a pinned llama.cpp (or similar) commit and build flags. One ISA class must be named, for example x86-64 with AVX2 only, or "Apple M4 Metal" only.
3. **Make decoding deterministic.**
   - Greedy (argmax) decoding with a fixed tie-break, or sampling from a committed seed, for example the drand beacon of that bell.
   - Batch size 1 and a single slot, with a fixed thread count (or a backend shown to be thread-count-invariant).
   - No speculative decoding, unless it is exact and verified.
4. **Commit to the inputs.** The prompt is built only from public chain state and the beacon, as §7.1 already requires for Shade inputs. The prompt template is part of the policy code hash.
5. **Make the outputs mechanical.** Constrain output to a small menu, such as an index into the legal actions. Map invalid output to a fixed fallback, so a model error cannot become an illegal move.
6. **Run verifiers on the same architecture.** Otherwise mismatches are expected rather than evidence of fraud (S16). In practice that means one reference environment, such as a rented H100 image or a CPU container, which anyone can rent to re-run.

### 4.3 Cost and practicality for Wylls [estimate]

- **A small model on a pinned CPU container is the most portable option.** Gemma 4 E4B or a 3–4B-active MoE are examples. At 4-bit, CPU-only generation might run at roughly 10–30 tok/s [estimate]. With ~2K-token prompts, every replayed decision costs seconds of CPU, and a verifier must re-run every Shade's every decision for a season. That is feasible for a few hundred Shades at bell granularity, but it makes verification hundreds of times more expensive than replaying deterministic policy code [estimate].
- **The model becomes consensus-critical.** Changing the model, runtime or ISA mid-season breaks replay.
- **An LLM-chosen move is harder to audit for fairness than a committed rule set.** The operator could tune prompts before genesis to favour a faction, and nobody can read intent out of weights. A deterministic policy can at least be read and simulated in full.

---

## 5. Implications for the AI-agent selling point [estimate]

1. **Keep §7 as it is.** Shades decide with committed deterministic code, and an LLM writes only chat flavour, which has no game effect. Re-running LLM decisions is possible (§4.2) but expensive, fragile across hardware, and harder to audit.
2. **One verifiable middle path: the LLM as a policy author, not a decider.**
   - Before genesis, or at committed checkpoints, an LLM (Claude Opus/Sonnet or a local Gemma 4) writes or tunes the **policy parameters or code** of each Shade.
   - The result is hashed into `roster_root` / `policy_code_hash`.
   - The verifier replays plain code, so it is cheap and exact.
   - This makes a true claim possible ("the Shades' strategies were designed by AI") without putting an LLM in consensus.
3. **The real AI-agent pitch: bring your own agent.** Players run agents through the public SDK/MCP:
   - on their own Claude key (the old `llm-agent.mjs` pattern, about $1–3 per agent-day on Sonnet/Haiku per W3);
   - or on a **free local model** (Gemma 4 / Qwen3.6 / gpt-oss through an OpenAI-compatible local server);
   - or on a cheap hosted open model (under $0.15 per agent-day per W3).

   The operator pays nothing for player agents. Agents play under the same rules and quotas as humans, so the chain keeps verifying only the moves, never the minds.
4. **Operator-paid AI stays to flavour and commentary.**
   - Haiku 4.5 (or Sonnet 5.5 with thinking off) for chat lines, about $60 per 100K lines.
   - Batch API for heralds and recaps.
   - Or a local Gemma 4 26B A4B on the Mac, near-free with a Claude fallback.
   - The advisor's `claude-haiku-4-5-20251001` pin needs a watch on the deprecation page.
5. **Open-model choice to test first (no downloads were made):**
   - Gemma 4 26B A4B (Apache 2.0, about 16 GB, about 70–100 tok/s estimated);
   - Qwen3.6-35B-A3B (Apache 2.0) for Japanese chat and simple tool use;
   - gpt-oss-120b if strategy strength matters and Japanese is acceptable.

## 6. To check before relying on this

- Measure real tok/s for the candidates on this Mac once a runtime is approved (llama.cpp or MLX), including prefill at 2K/8K tokens and 4–16 concurrent streams.
- Check Japanese quality of gpt-oss-120b vs Gemma 4 vs Qwen3.6 on real game chat.
- Run a determinism test: the same prompt 1,000 times on one runtime build, batch 1, greedy, then on a second machine of the same class and of a different class. Record hash agreement.
- Re-read the Anthropic deprecation page monthly for `claude-haiku-4-5-20251001`.
- Confirm the M4 Max variant (16-core CPU / 40-core GPU / 546 GB/s) from *About This Mac*.
- Secondary sources (S17, S19, S21, S22, S25, S28, S31) are aggregators or blogs. Their numbers are indicative only.

---

## Sources (all read 2026-10-01)

- S1 Anthropic, Pricing — https://platform.claude.com/docs/en/about-claude/pricing
- S2 Anthropic, Model deprecations — https://platform.claude.com/docs/en/about-claude/model-deprecations
- S3 Anthropic, Claude Sonnet 5.5 overview — https://platform.claude.com/docs/en/models/sonnet-5-5/overview ; Models overview — https://platform.claude.com/docs/en/about-claude/models/overview
- S4 Anthropic prompt-caching minimums and economics (Claude API skill reference, cached 2026-06-24, consistent with S1) — https://platform.claude.com/docs/en/build-with-claude/prompt-caching
- S5 Google, Gemma 4 model card — https://ai.google.dev/gemma/docs/core/model_card_4
- S6 Google, Gemma releases — https://ai.google.dev/gemma/docs/releases
- S7 Google, Gemma Terms of Use (last updated 2026-04-01) — https://ai.google.dev/gemma/terms
- S8 Hugging Face public model API (`/api/models/<id>`: licence, parameter count, repo creation date) for google/gemma-4-*, google/gemma-3-27b-it, Qwen/Qwen3.6-35B-A3B, Qwen/Qwen3.6-27B, Qwen/Qwen3.8-27B, Qwen/Qwen3.5-122B-A10B, openai/gpt-oss-20b/120b, mistralai/Mistral-Small-4-119B-2603, meta-llama/Llama-4-Scout-17B-16E-Instruct
- S9 OpenRouter public model list — https://openrouter.ai/api/v1/models
- S10 OpenAI, Introducing gpt-oss — https://openai.com/index/introducing-gpt-oss/ ; model card https://huggingface.co/openai/gpt-oss-120b
- S11 Qwen, Qwen3.6-35B-A3B — https://qwen.ai/blog?id=qwen3.6-35b-a3b ; https://huggingface.co/Qwen/Qwen3.6-35B-A3B
- S12 llama.cpp, Performance on Apple Silicon M-series (Discussion #4167) — https://github.com/ggml-org/llama.cpp/discussions/4167
- S13 "Native LLM and MLLM Inference at Scale on Apple Silicon" (arXiv 2601.19139, M4 Max 128 GB) — https://arxiv.org/html/2601.19139v1
- S14 Olares, "Which Hardware Runs Local AI Best? 6 Setups Compared" (Mac Studio M4 Max 64 GB, 2025-11) — https://www.olares.com/blog/local-ai-hardware-performance-benchmarking/
- S15 LM Studio mlx-engine issue #337 (Gemma 4 26B-A4B reasoning never terminates on MLX) — https://github.com/lmstudio-ai/mlx-engine/issues/337
- S16 EigenAI: Deterministic Inference, Verifiable Results (arXiv 2602.00182, 2026-01-30) — https://arxiv.org/html/2602.00182
- S17 Presenc AI, Local LLM tokens-per-second benchmarks 2026 (aggregated bands, 2026-05) — https://presenc.ai/research/local-llm-tokens-per-second-benchmarks-2026
- S18 llama.cpp, guide: running gpt-oss with llama.cpp (Discussion #15396; M2 Ultra / M3 Ultra numbers) — https://github.com/ggml-org/llama.cpp/discussions/15396
- S19 GPU rental prices September 2026 — https://www.thundercompute.com/blog/nvidia-h100-pricing ; https://dev.to/fastgpu/what-it-costs-to-rent-an-h100-b200-or-rtx-4090-in-september-2026-live-prices-from-28-gpu-clouds-12n3
- S20 Google blog, Gemma 4 — https://blog.google/innovation-and-ai/technology/developers-tools/gemma-4/
- S21 Qwen 3.5 overviews (secondary) — https://www.morphllm.com/qwen-3-5 ; https://codersera.com/blog/qwen-3-8-model-lineup-2026/
- S22 Mistral Small 4 coverage (secondary) — https://letsdatascience.com/news/mistral-releases-mistral-small-4-model-8731b156
- S23 Meta, Llama 4 Community License — https://dev.meta.ai/llama/llama4/license/
- S24 Meta, Llama 4 Acceptable Use Policy — https://dev.meta.ai/llama/llama4/use-policy/
- S25 Reports on Meta's move away from open Llama releases (secondary) — https://siliconangle.com/2026/04/06/report-meta-developing-open-source-versions-upcoming-ai-models/
- S26 Thinking Machines Lab, Defeating Nondeterminism in LLM Inference (2025-09) — https://thinkingmachines.ai/blog/defeating-nondeterminism-in-llm-inference/
- S27 vLLM docs, Batch Invariance — https://docs.vllm.ai/en/latest/features/batch_invariance/
- S28 Independent measurement of vLLM batch-invariance cost (secondary) — https://github.com/AshrithaG/batch-invariance
- S29 llama.cpp issue #7052, non-deterministic output with multiple slots — https://github.com/ggml-org/llama.cpp/issues/7052
- S30 react-native-executorch issue #1379 (MLX logit divergence Mac vs iPhone) — https://github.com/software-mansion/react-native-executorch/issues/1379
- S31 Blog, MLX determinism problem (secondary) — https://adityakarnam.com/mlx-non-determinism-apple-silicon/
- S32 "Deterministic LLM Inference Across GPU Kernels: Power-of-Two INT8 Quantization Scales…" (arXiv 2609.00363, 2026-08-25) — https://arxiv.org/abs/2609.00363
- S33 opML: Optimistic Machine Learning on Blockchain (arXiv 2401.17555) — https://arxiv.org/abs/2401.17555 ; https://github.com/ora-io/opml
- Repo facts (read-only grep): `permutation-gateway/src/advisor.mjs` line 13 pins `claude-haiku-4-5-20251001`, `max_tokens: 160`, no caching. `permutation-gateway/agents/llm-agent.mjs` defaults to `claude-sonnet-5`, caches tools and system, `max_tokens: 2048`, a 2,000,000 input-token budget, no `tool_choice` and no `temperature`.
