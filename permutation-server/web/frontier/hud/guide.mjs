// The guide on the map (UI plan G1, G2; Civ VII's advisors): the onboarding
// step points at its target — the nearest barbarian camp for the first
// march (with a ready host and the destination filled in, one press from
// "seal and depart"), the holding for the build and the scouts, free land
// for settling — and the viewer chooses how loud the guide is:
//   all    the guide card, its map highlight, every to-do item
//   warn   no card, no highlight; the to-do pill only for warnings
//   off    no card, no highlight, no pill (the rail's list stays)
import { L } from '../../lang.mjs';
import { activeHolding } from '../fstate.mjs';
import { onboardingState, factsOf } from '../onboarding.mjs';
import { hostRows } from '../screens/host.mjs';
import { SCOUT, SETTLER } from '../fland.mjs';
import { ringOf } from '../fgeo.mjs';

export const GUIDE_LEVELS = Object.freeze(['all', 'warn', 'off']);
export const GUIDE_TEXT = { all: () => L`すべて`, warn: () => L`警告だけ`, off: () => L`オフ` };
/** The to-do kinds that are warnings (shown at "warn"). */
export const WARN_KINDS = Object.freeze(new Set(['incoming', 'dormant', 'full', 'settle']));

export const guideLevel = FS => (GUIDE_LEVELS.includes(FS.ui?.guide) ? FS.ui.guide : 'all');

const dist = (a, b) => { const dq = a.q - b.q, dp = a.p - b.p; return (Math.abs(dp) + Math.abs(dq) + Math.abs(dp + dq)) / 2; };

/**
 * The current step's target: `{step, kind, p, q, tile?, host?}` or null —
 * `march` the nearest loaded camp and the first ready host (not a scout or settler),
 * `build` / `scout` the active holding, `settle` free land (the settle lens).
 */
export function guideTarget(FS) {
  if (guideLevel(FS) !== 'all' || FS.mode !== 'play') return null;
  let st;
  try { st = onboardingState(factsOf(FS), FS.ui?.dismissed ?? [], FS.clock ?? null); } catch { return null; }
  if (st.dismissed) return null;
  const step = st.current, h = activeHolding(FS);
  if (step === 'join' && FS.land?.stage === 'joined') return { step, kind: 'settle' };
  if (!h) return null;
  if (step === 'build') return { step, kind: 'build', p: h.p, q: h.q, tile: h.tile };
  if (step === 'scout') return { step, kind: 'scout', p: h.p, q: h.q, tile: h.tile };
  if (step === 'march') {
    let best = null;
    for (const env of FS.provinces?.values() ?? []) {
      const pr = env.province, c = pr?.camp;
      if (c?.state !== 1) continue;
      const d = dist(pr, h) * 100 + ringOf(pr.p, pr.q);
      if (!best || d < best.d) best = { d, p: pr.p, q: pr.q, tile: c.tile };
    }
    const host = hostRows(FS).find(r => r.state === 1 && !r.inTransit && !r.pending && r.unit !== SCOUT && r.unit !== SETTLER) ?? null;
    return best ? { step, kind: 'march', p: best.p, q: best.q, tile: best.tile, host: host ? String(host.id) : null } : null;
  }
  return null;
}

/** The guide card's button for a target. */
export function goText(t) {
  return { march: t.host ? L`この野営地へ進軍を準備する` : L`野営地を地図で見る`, build: L`建設のパネルへ`, scout: L`拠点と軍勢を見る`, settle: L`入植できる地を地図で見る` }[t.kind] ?? '';
}

/** The map's label at the target. */
export const guideLabel = t => ({ march: L`ガイド：蛮族の野営地`, build: L`ガイド：あなたの拠点`, scout: L`ガイド：ここから探索`, settle: L`ガイド：空き区画` }[t.kind] ?? L`ガイド`);
