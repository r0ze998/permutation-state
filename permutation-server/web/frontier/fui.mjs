// UI preferences of one season on this device (contract §9.1, key
// `ps-fui:<cluster>:<program>:<season>`): the fog switch, the map's level of
// detail, the open tab, dismissed hints, and the sites of the last site
// ticket (so a displaced or exhausted ticket can be filed again with one
// tap, I-47). Convenience only: nothing here is chain state, a missing or
// damaged record reads as the defaults, and a storage that refuses writes
// only costs the preference.
export const UI_PREFIX = 'ps-fui:';
export const TABS = Object.freeze(['map', 'holding', 'hosts', 'marches', 'more']);
// `ticketSeen`: the herald has shown the last ticket on chain (W6-D: until then a
// Citizen still at "joined" is a ticket not yet folded, not one that ended).
// `lastTicketBell`: the bell the last ticket was filed at (integ-W6 review: a
// ticket can end between two polls without the page seeing it open, so the
// offer also counts it as seen `TICKET_SEEN_BELLS` after the filing bell).
export const DEFAULTS = Object.freeze({ v: 1, fog: true, lod: 'world', tab: 'map', dismissed: [], lastTicket: null, ticketSeen: true, lastTicketBell: null, autoPan: false });

/** `ps-fui:<cluster>:<program>:<season>`. */
export const uiKey = ({ cluster, programId, seasonId }) => `${UI_PREFIX}${cluster}:${programId}:${seasonId}`;

const memory = new Map();
/** localStorage with an in-memory fallback (private windows, tests). */
export const uiStorage = {
  get(k) {
    try { const v = globalThis.localStorage?.getItem(k); if (v != null) return v; } catch { /* unavailable */ }
    return memory.get(k) ?? null;
  },
  set(k, v) {
    memory.set(k, v);
    try { globalThis.localStorage.setItem(k, v); return true; } catch { return false; }
  },
};

const isSite = s => s && Number.isInteger(s.p) && Number.isInteger(s.q) && Number.isInteger(s.site) && s.site >= 0 && s.site < 12;

/** The stored preferences with every field checked (unknown or damaged fields → defaults). */
export function loadUi(storage, key) {
  let v = null;
  try { v = JSON.parse(storage.get(key) ?? 'null'); } catch { v = null; }
  if (!v || typeof v !== 'object' || v.v !== 1) return { ...DEFAULTS, dismissed: [] };
  return {
    v: 1,
    fog: typeof v.fog === 'boolean' ? v.fog : DEFAULTS.fog,
    lod: ['world', 'province', 'tile'].includes(v.lod) ? v.lod : DEFAULTS.lod,
    tab: TABS.includes(v.tab) ? v.tab : DEFAULTS.tab,
    dismissed: Array.isArray(v.dismissed) ? v.dismissed.filter(x => typeof x === 'string').slice(0, 64) : [],
    lastTicket: Array.isArray(v.lastTicket) && v.lastTicket.length >= 1 && v.lastTicket.length <= 3 && v.lastTicket.every(isSite)
      ? v.lastTicket.map(({ p, q, site }) => ({ p, q, site })) : null,
    ticketSeen: typeof v.ticketSeen === 'boolean' ? v.ticketSeen : DEFAULTS.ticketSeen,
    lastTicketBell: Number.isInteger(v.lastTicketBell) && v.lastTicketBell >= 0 ? v.lastTicketBell : null,
    autoPan: typeof v.autoPan === 'boolean' ? v.autoPan : DEFAULTS.autoPan,
  };
}

/** Save a patch over the stored preferences; returns the new preferences. */
export function saveUi(storage, key, patch) {
  const merged = { ...loadUi(storage, key), ...patch, v: 1 };
  storage.set(key, JSON.stringify(merged));
  return loadUi(storage, key);
}
