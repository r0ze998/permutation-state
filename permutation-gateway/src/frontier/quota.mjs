// Sponsorship quotas (contract §8.3, D4): per citizen and game day,
//
//   transactions  40 a day on game days 0–6, then 20; unused allowance carries over up to a burst of 60
//   lamports      ≤ 24 Depart escrows at the largest tip preset + the Citizen's rent (Join) + the
//                 Holding's rent escrow (FileTicket, I-47)
//
// A settle shape is charged to its requester (a session key that signed the
// request, else the client-address bucket), never to the citizen it names
// (I-51). Nothing is charged for a transaction whose simulation failed: the
// routes hold one transaction and the kind's allowance while they check,
// give it all back on a refusal, and keep only what they send (the lamports
// the simulation really moved). The game
// day is the Clock's (`day = ⌊(now − genesis_ts) / 86,400⌋`), never the
// wall clock; it resets at the game midnight.
import { RouteError } from '../routes/errors.mjs';

export const QUOTA = Object.freeze({ earlyPerDay: 40, latePerDay: 20, earlyDays: 7, burst: 60, departsPerDay: 24 });
export const GAME_DAY_SECS = 86_400;

/** Sponsored transactions a key gets on game day `day`. */
export const dailyTxs = day => (day < QUOTA.earlyDays ? QUOTA.earlyPerDay : QUOTA.latePerDay);

/** The game day of Clock time `now` for a season that started at `genesisTs` (0 before genesis). */
export const gameDay = (now, genesisTs) => (Number(now) <= Number(genesisTs) ? 0 : Math.floor((Number(now) - Number(genesisTs)) / GAME_DAY_SECS));

/** When game day `day` ends (Clock seconds). */
export const dayEnd = (day, genesisTs) => Number(genesisTs) + (day + 1) * GAME_DAY_SECS;

export class QuotaBook {
  /**
   * `store`: `{state, save()}` (config.mjs createStateStore, or an object
   * in memory); its `state.quota` holds key → {day, left, lamports}.
   */
  constructor({ store = { state: {}, save() {} } } = {}) {
    this.store = store;
    this.store.state ??= {};
    this.store.state.quota ??= {};
  }

  get entries() { return this.store.state.quota; }

  /** The key's entry as of `day` (not stored). */
  view(key, day) {
    const e = this.entries[key];
    if (!e) return { day, left: dailyTxs(day), lamports: 0 };
    if (e.day >= day) return { ...e };
    let left = e.left;
    for (let d = e.day + 1; d <= day && left < QUOTA.burst; d++) left = Math.min(QUOTA.burst, left + dailyTxs(d));
    return { day, left, lamports: 0 };
  }

  /** `{left, lamportsLeft, resetsAt}` for GET /f/quota and GET /f/relay. */
  status(key, { day, lamportsCap, genesisTs }) {
    const v = this.view(key, day);
    return { left: v.left, lamportsLeft: String(Math.max(0, Number(lamportsCap) - v.lamports)), resetsAt: dayEnd(day, genesisTs) };
  }

  /** Throws 429 QuotaExceeded {retryAt} unless one more transaction moving `lamports` fits. */
  check(key, { day, lamports = 0, lamportsCap, genesisTs }) {
    const v = this.view(key, day);
    const retryAt = dayEnd(day, genesisTs);
    if (v.left < 1) throw new RouteError(429, 'the sponsored-transaction quota for this game day is used up', 'QuotaExceeded', { retryAt });
    if (lamportsCap !== undefined && v.lamports + Number(lamports) > Number(lamportsCap)) {
      throw new RouteError(429, 'the sponsored-lamport quota for this game day is used up', 'QuotaExceeded', { retryAt });
    }
  }

  /** Spend one transaction and `lamports` of the key's quota (held while a transaction is checked; `refund` gives it back). */
  charge(key, { day, lamports = 0 }) {
    const v = this.view(key, day);
    this.entries[key] = { day, left: v.left - 1, lamports: v.lamports + Number(lamports) };
    this.prune(day);
    this.store.save?.();
  }

  /** Give back `txs` transactions and `lamports` held by `charge` (a refused transaction, or the unused part of an allowance). */
  refund(key, { day, lamports = 0, txs = 1 }) {
    const v = this.view(key, day);
    const l = BigInt(v.lamports) - BigInt(lamports);
    this.entries[key] = { day, left: v.left + txs, lamports: Number(l > 0n ? l : 0n) };
    this.store.save?.();
  }

  /** Forget entries that would be full again (their allowance has refilled to the burst). */
  prune(day) {
    for (const [k, e] of Object.entries(this.entries)) if (day - e.day >= 2 && this.view(k, day).left >= QUOTA.burst) delete this.entries[k];
  }
}
