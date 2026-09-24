#!/usr/bin/env node
// Reference agent 2: a language model plays through tools.
//
//   node agents/llm-agent.mjs --name Hypatia --server http://127.0.0.1:4185 --gateway http://127.0.0.1:4191 \
//     [--key-file .local/anthropic-key] [--model claude-sonnet-5] [--lang en] [--ticks N] [--max-input-tokens 2000000]
//
// Each tick the model gets its fogged state summary and the same tools an
// MCP client gets (previews, find_path, validate_orders) and must end with
// submit_orders. The batch is validated before it is accepted; errors go
// back to the model so it can fix them. The model's own rationale is
// committed with the batch and revealed after the tick — so spectators see
// what it claimed to think, and can check it was fixed before the outcome.
//
// The API key comes from --key-file (default .local/anthropic-key, which is
// git-ignored; the file holds only the key) or ANTHROPIC_API_KEY. It is sent
// only to ANTHROPIC_BASE_URL (default https://api.anthropic.com) and never
// logged. Token use is logged per tick; past --max-input-tokens the agent
// stops calling the model and holds (empty batches) until the season ends.
import { existsSync, readFileSync } from 'node:fs';
import { parseArgs, runAgent } from './runner.mjs';
import { TOOLS, callTool, ORDER_REFERENCE, RULES_BRIEF } from '../client/src/tools.mjs';
import { summarize } from '../client/src/summary.mjs';

const args = parseArgs(process.argv, { name: 'Hypatia', model: process.env.PS_MODEL || 'claude-sonnet-5', lang: 'ja', maxSteps: '10', keyFile: '.local/anthropic-key', maxInputTokens: '2000000' });
const API = (process.env.ANTHROPIC_BASE_URL || 'https://api.anthropic.com').replace(/\/$/, '');
const KEY = (existsSync(args.keyFile) ? readFileSync(args.keyFile, 'utf8') : process.env.ANTHROPIC_API_KEY || '').trim();
if (!KEY) {
  console.error(`llm-agent: put your API key in ${args.keyFile} (or set ANTHROPIC_API_KEY). The rule-based agent (agents/rule-agent.mjs) needs no key.`);
  process.exit(2);
}
const usage = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, calls: 0 };
// Errors that retrying cannot fix (no credit, bad key, no access, budget):
// stop calling the model and hold, with an honest rationale.
let unavailable = null;
const FATAL = /credit balance|invalid x-api-key|authentication|permission|not_found_error|token budget reached/i;

const SYSTEM = `You are ${args.name}, an AI member of one nation in PERMUTATION STATE, playing one tick at a time.
${RULES_BRIEF}

${ORDER_REFERENCE}

How to play a tick:
- Read the state you are given ("you.officesHeld" says which offices you hold). Use previews (preview_city, preview_research, preview_unit, find_path) only when you need facts; do not guess ids or coordinates.
- If you hold offices: spend each office's budget on what matters most (research queued, cities producing, settlers founding, scouts exploring, defence, only fights the forecast favours). Adopt good proposals of other members (submit_orders.adopt).
- For offices you do not hold, propose what you would do (propose), and support workable proposals (govern Support). In the vote window, vote (govern Vote).
- Call validate_orders if unsure, then finish with exactly one submit_orders call (an empty one if you hold no office). Its rationale is published after the tick: write ${args.lang === 'en' ? 'one or two plain English sentences' : '日本語で1〜2文'} on why, honestly.
- You have a few tool calls per tick; be decisive.`;

async function messages(body) {
  if (usage.input + usage.cacheRead + usage.cacheWrite >= Number(args.maxInputTokens)) throw new Error(`token budget reached (${args.maxInputTokens} input tokens)`);
  const res = await fetch(`${API}/v1/messages`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-api-key': KEY, 'anthropic-version': '2023-06-01' },
    body: JSON.stringify(body),
  });
  const json = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(`model API ${res.status}: ${json.error?.message ?? JSON.stringify(json).slice(0, 200)}`);
  const u = json.usage || {};
  usage.calls++;
  usage.input += u.input_tokens || 0;
  usage.output += u.output_tokens || 0;
  usage.cacheRead += u.cache_read_input_tokens || 0;
  usage.cacheWrite += u.cache_creation_input_tokens || 0;
  return json;
}

// The model may call every tool except get_state (it gets the state up front);
// submit_orders is intercepted: it validates and hands the batch to the runner.
const tools = TOOLS.filter(t => t.name !== 'get_state').map(t => ({ name: t.name, description: t.description, input_schema: t.inputSchema }));
// The rules and tool definitions are the same every call: cache them.
tools[tools.length - 1].cache_control = { type: 'ephemeral' };
const system = [{ type: 'text', text: SYSTEM, cache_control: { type: 'ephemeral' } }];

async function decide({ game, view, log }) {
  if (unavailable) return { orders: [], rationale: args.lang === 'en' ? `The model is unavailable (${unavailable}); holding.` : `モデルを使えないため待機します（${unavailable}）。` };
  const ctx = { game, policy: `llm-agent/${args.model}`, view };
  const deadline = Date.now() + Math.max(5, (view.secondsLeft ?? 20) - 4) * 1000;
  const msgs = [{ role: 'user', content: `Tick ${view.tick} of ${view.ticks}. ${Math.round(view.secondsLeft)} seconds left.\nYour state:\n${JSON.stringify(summarize(view, { member: game.member }))}` }];
  for (let step = 0; step < Number(args.maxSteps) && Date.now() < deadline; step++) {
    let r;
    try {
      r = await messages({ model: args.model, max_tokens: 2048, system, tools, messages: msgs });
    } catch (e) {
      log(`model call failed: ${e.message}`);
      if (FATAL.test(e.message)) {
        unavailable = /credit balance/i.test(e.message) ? 'API credit exhausted' : /budget/.test(e.message) ? 'token budget reached' : 'API key rejected';
        log(`the model is unavailable (${unavailable}): holding for the rest of the season without calling it`);
        return { orders: [], rationale: args.lang === 'en' ? `The model is unavailable (${unavailable}); holding.` : `モデルを使えないため待機します（${unavailable}）。` };
      }
      break;
    }
    msgs.push({ role: 'assistant', content: r.content });
    const uses = r.content.filter(b => b.type === 'tool_use');
    if (!uses.length) break;
    const results = [];
    let batch = null;
    for (const u of uses) {
      let out;
      if (u.name === 'submit_orders') {
        const orders = Array.isArray(u.input.orders) ? u.input.orders : [];
        const check = await game.validate(orders).catch(e => ({ ok: false, error: e.message }));
        const held = game.myOffices(view);
        const bad = (check.offices || []).filter(o => held.includes(o.role) && o.error);
        if (!bad.length) { batch = { orders, rationale: String(u.input.rationale || ''), adopt: u.input.adopt || {} }; out = { ok: true, offices: check.offices, warnings: check.warnings }; }
        else out = { ok: false, error: bad.map(o => `${o.role}: ${o.error}`).join('; '), warnings: check.warnings, hint: 'fix the batch and call submit_orders again' };
      } else {
        out = await callTool(ctx, u.name, u.input || {});
      }
      results.push({ type: 'tool_result', tool_use_id: u.id, content: JSON.stringify(out).slice(0, 8000), is_error: Boolean(out?.error || out?.ok === false) });
    }
    if (batch) {
      log(`model used ${step + 1} steps; tokens so far: ${usage.calls} calls, input ${usage.input} + cache read ${usage.cacheRead} + cache write ${usage.cacheWrite}, output ${usage.output}`);
      return batch;
    }
    msgs.push({ role: 'user', content: results });
  }
  return { orders: [], rationale: args.lang === 'en' ? 'No decision within the time limit; holding.' : '時間内に判断がまとまらなかったため、今回は待機します。' };
}

runAgent({ name: args.name, policy: `llm-agent/${args.model}`, decide, args }).catch(e => { console.error(e); process.exit(1); });
