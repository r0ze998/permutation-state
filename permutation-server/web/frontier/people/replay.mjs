// The season replay's people (UI plan F3: "the replay VI never had", with
// the replay page's own data). The replay page (replay/main.mjs, the M1
// chat's) reads per-bell overviews and, at tile detail, the province
// envelopes of the bell shown; this adapter turns exactly that into the
// map's `people()` input: the holders' names founded by that bell (the
// roster), the tier names, and the battle scenes of the clashes resolved in
// that bell (the envelope's ClashInputs, the province before and after).
//
//   import { createReplayPeople } from '../people/replay.mjs';
//   const rp = createReplayPeople({ roster, rings: () => ringsOpen });
//   source.people = () => rp.at(S.shown, { envelopes: visibleEnvelopes(S.shown), before: (p, q) => S.provinces.at(p, q, S.shown - 1)?.province });
//   // when the playhead enters a bell: rp.enter(b) — its clashes start playing
//
// Nothing is invented: a bell without records shows no walkers; a battle
// plays only where the bell's ClashInputs exist.
import { L, fmtNum } from '../../lang.mjs';
import { TIERS } from '../fi18n.mjs';
import { namer } from './scene.mjs';
import { battleScene, startBattle, battleLive, BATTLE_SPEEDS } from './battle.mjs';

/** Battle scenes of one bell from envelopes `[{bell, province, inputs}]` and the province before (a function). */
export function replayBattles(bell, envelopes, before = () => null) {
  const out = [];
  for (const env of envelopes ?? []) {
    if (!env || env.bell !== bell || !env.inputs || !env.province) continue;
    const s = battleScene({ p: env.province.p, q: env.province.q, bell, inputs: env.inputs, before: before(env.province.p, env.province.q) ?? null, after: env.province });
    if (s) out.push(s);
  }
  return out;
}

/** The replay's people source: `at(bell, {envelopes, before, now})` → the map's people input; `enter(bell)` starts its battles. */
export function createReplayPeople({ roster = null, speed = BATTLE_SPEEDS.fast, clock = () => (globalThis.performance?.now?.() ?? Date.now()) / 1000 } = {}) {
  let plays = [];
  let entered = null;
  let pending = null;
  return {
    /** The playhead entered `bell`: its battles start at the next `at` that has their envelopes. */
    enter(bell) { pending = bell; },
    at(bell, { envelopes = [], before = () => null, life = null, now = 0 } = {}) {
      const t = clock();
      if (pending !== null && pending !== entered) {
        const scenes = replayBattles(pending, envelopes, before);
        if (scenes.length) { plays = [...plays.filter(b => battleLive(b, t)), ...scenes.map(s => startBattle(s, t, speed))]; entered = pending; pending = null; }
      }
      plays = plays.filter(b => battleLive(b, t));
      return {
        departures: [], explores: [], own: [], bell, now, life,
        nameOf: namer(roster, { bell }),
        tierName: x => TIERS[x] ?? '',
        lordOf: null,
        battles: plays,
        lossText: n => L`−${fmtNum(n)} 兵`,
        fateText: f => ({ Stays: L`持ちこたえた`, Withdrew: L`隣へ退いた`, Bounced: L`押し戻された`, Retreated: L`撤退した`, Destroyed: L`壊滅` })[f] ?? null,
      };
    },
    playing: () => plays.length,
  };
}
