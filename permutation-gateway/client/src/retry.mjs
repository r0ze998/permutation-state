// One retry helper for every "try again" loop (RPC sends, polls, agents),
// and the error classes the loops retry on.

export const sleep = ms => new Promise(r => setTimeout(r, ms));

/**
 * Run `fn(attempt)` until it resolves, at most `attempts` times (Infinity is
 * allowed). Between tries wait `delayMs × backoff^attempt` (capped at
 * `maxDelayMs`). An error for which `retryIf(error, attempt)` is false, or
 * the last attempt's error, is thrown. `onRetry(error, attempt, waitMs)` is
 * called before each wait.
 */
export async function retry(fn, { attempts = 3, delayMs = 500, backoff = 1, maxDelayMs = 30_000, retryIf = () => true, onRetry } = {}) {
  for (let attempt = 0; ; attempt++) {
    try {
      return await fn(attempt);
    } catch (e) {
      if (attempt + 1 >= attempts || !retryIf(e, attempt)) throw e;
      const wait = Math.min(delayMs * backoff ** attempt, maxDelayMs);
      onRetry?.(e, attempt, wait);
      await sleep(wait);
    }
  }
}

const PENDING = Symbol('pending');

/**
 * Poll `fn(attempt)` until it returns something other than null/undefined;
 * errors count as "not yet". Returns null if it never does.
 */
export async function poll(fn, { attempts = 10, delayMs = 500, backoff = 1, maxDelayMs = 30_000 } = {}) {
  try {
    return await retry(async attempt => {
      const v = await fn(attempt).catch(() => null);
      if (v === null || v === undefined) throw PENDING;
      return v;
    }, { attempts, delayMs, backoff, maxDelayMs });
  } catch (e) {
    if (e === PENDING) return null;
    throw e;
  }
}

const describe = e => `${e?.message ?? e} ${(e?.logs || []).join(' ')}`;

/** A failure of the RPC transport (rate limit, dropped connection, timeout), not of the transaction: worth retrying. */
export const isTransientRpcError = e =>
  /\b429\b|Too Many Requests|rate limit|ECONNRESET|ETIMEDOUT|socket hang up|fetch failed|timed? ?out|Service Unavailable|Bad Gateway|Gateway Timeout/i.test(e?.message ?? String(e));

/** The transaction ran out of compute or heap: the same work split into smaller parts may fit. */
export const isHeavyError = e => /exceeded CUs|ComputationalBudgetExceeded|ProgramFailedToComplete|out of memory|memory allocation failed/i.test(describe(e));
