// The march card's forecast (UI plan B3; Civ's combat preview with the
// predicted damage range): the rules module resolves the clash at the
// destination with the viewer's host arriving — its stance, retreat ratio,
// troops and stamina — against the defenders the page holds, under several
// random seeds. The card shows the range of troops left (worst to best) and
// how often each fate came up. Honest about what it cannot know: other
// arrivals are sealed until their bell, and the defenders are this
// moment's, not the bell's start.
import { buildClashArgs, resolveArgs } from '../screens/report.mjs';
import { retreatBps, RULES, staminaAt } from '../fmarch.mjs';
import { sha256 } from '../../sdk/sha256.mjs';

export const FORECAST_RUNS = 12;

/** The ClashInputs-shaped input with the viewer's host as the only arrival. */
export function forecastInputs(c, faction) {
  const h = c.host, bell = c.arriveBell;
  // the march's cost on the way (base + per hex of the planned route)
  const stamina = Math.max(0, staminaAt(h, bell) - (RULES.MARCH_STAMINA_BASE + RULES.MARCH_STAMINA_PER_HEX * (c.route?.dirs?.length ?? 0)));
  const retreat = c.retreat === 'never' ? 0 : retreatBps(c.retreat, c.ratio) ?? 0;
  return { p: c.dest.p, q: c.dest.q, bell, arrivals: [{ present: 1, hostId: BigInt(h.id), faction, unit: h.unit, troops: h.troops * 1000, stamina, tile: c.dest.tile, stance: c.stance ?? 0, retreat, dealt: 0 }] };
}

/**
 * `{ok: true, runs, troops: {min, max, start}, fates: {kind: n}, certain}` or
 * `{ok: false, why}`. `kernel` is the loaded frontier.wasm; `province` the
 * destination's Province the page holds.
 */
export function forecast(kernel, c, province, faction, runs = FORECAST_RUNS) {
  if (!kernel || !c?.host || !c.dest || !province || !Number.isInteger(c.arriveBell)) return { ok: false, why: 'NoInput' };
  const inputs = forecastInputs(c, faction);
  const id = String(c.host.id);
  let min = Infinity, max = -Infinity, certain = true;
  const fates = {};
  for (let i = 0; i < runs; i++) {
    const seed = sha256('PS-FRONTIER-FORECAST', new TextEncoder().encode(`${id}:${c.dest.p},${c.dest.q},${c.dest.tile}:${c.arriveBell}:${i}`));
    const built = buildClashArgs({ province, inputs, bell: c.arriveBell, seed });
    if (!built.ok) return { ok: false, why: built.why };
    certain &&= built.certain;
    // a march inside its own province: the host leaves its place among the residents to arrive
    built.args.residents = built.args.residents.filter(x => String(x.id) !== id);
    const r = resolveArgs(kernel, built.args);
    if (!r.ok) return { ok: false, why: r.why, reason: r.reason ?? null };
    const me = r.outcome.fighters.find(f => f.arrival && String(f.id) === id);
    if (!me) return { ok: false, why: 'NoFighter' };
    const left = Math.floor(Number(me.troops) / 1000);
    min = Math.min(min, left); max = Math.max(max, left);
    fates[me.fate.kind] = (fates[me.fate.kind] ?? 0) + 1;
  }
  return { ok: true, runs, troops: { min, max, start: c.host.troops }, fates, certain };
}

/** The key a forecast is for (recompute when any input changes). */
export const forecastKey = c => (c?.host && c.dest ? `${c.host.id}|${c.dest.p},${c.dest.q},${c.dest.tile}|${c.arriveBell}|${c.stance}|${c.retreat}|${c.ratio}` : null);
