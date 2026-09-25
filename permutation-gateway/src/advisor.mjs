// Phrasing the operator AI members' answers with a language model (V5
// §18.8), when the operator has a key: the game server decides what the AI
// answers (by its temperament: the price, what binds); the model only puts it
// in words. Without a key, over the season's budget, or on any error the
// game server's own sentence is used. The operator pays (its key).
//
// The model is told never to claim to be a person: who the operator's AI
// members are is revealed at the end of the season, and it says so if asked.
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { LOCAL_DIR } from './config.mjs';

export const ADVISOR_MODEL = 'claude-haiku-4-5-20251001';

export function advisorKey(env = process.env) {
  if (env.ANTHROPIC_API_KEY) return env.ANTHROPIC_API_KEY;
  const f = path.join(LOCAL_DIR, 'anthropic-key');
  return existsSync(f) ? readFileSync(f, 'utf8').trim() : null;
}

export class Advisor {
  constructor({ key = advisorKey(), budget = 200, timeoutMs = 4000, fetchImpl = globalThis.fetch } = {}) {
    Object.assign(this, { key, budget, timeoutMs, fetchImpl, used: 0 });
  }

  get available() { return !!this.key && this.used < this.budget; }

  /** The answer to `draft.incoming` in the AI's voice, or `fallback`. */
  async phrase({ fallback, draft, nation }) {
    if (!this.available || !draft) return fallback;
    this.used++;
    const system = `You are one member of the nation ${nation} in PERMUTATION STATE, a strategy game of six nations. Answer another member's message in one or two short sentences, in the language they wrote in.
Your temperament: aggression ${draft.aggression}/100, greed ${draft.greed}/100, loyalty ${draft.loyalty}/100, openness ${draft.openness}/100.
What you mean to say (keep its substance): ${fallback}
Only a treasury contract binds you (OfferContract); do not promise anything else. Never claim to be a person. If asked whether you are an AI, say that who the operator's AI members are is revealed at the end of the season.`;
    try {
      // The signal aborts the request itself (and its body) at the timeout,
      // leaving no timer behind when the model answers in time.
      const res = await this.fetchImpl('https://api.anthropic.com/v1/messages', {
        method: 'POST',
        headers: { 'content-type': 'application/json', 'x-api-key': this.key, 'anthropic-version': '2023-06-01' },
        body: JSON.stringify({ model: ADVISOR_MODEL, max_tokens: 160, system, messages: [{ role: 'user', content: String(draft.incoming ?? '') }] }),
        signal: AbortSignal.timeout(this.timeoutMs),
      });
      if (!res.ok) return fallback;
      const body = await res.json();
      const text = body?.content?.find?.(c => c.type === 'text')?.text?.trim();
      return text ? text.slice(0, 280) : fallback;
    } catch {
      return fallback;
    }
  }
}
