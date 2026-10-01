// The life of each holding, from what its lord did (design session: "tie the
// walkers to real data, and let the players appear"). The chronicle is
// public: HARVEST, BUILD (item, done at), TRAIN (unit, n, done at), MUSTER,
// DEPART, EXPLORE and SETTLE name the holding (directly, or through the
// host id). The page folds every record it reads into one small record per
// holding, and the people layer (crowds.mjs) draws from it:
//   the lord        appears at a holding whose lord acted in the last
//                   LORD_BELLS bells, doing that thing (directing builders,
//                   drilling recruits, bringing in a harvest, leading a march)
//   builders        while a BUILD is under way (now < done_at)
//   recruits        while a TRAIN is under way
//   carriers        for a bell after a HARVEST, from the worked tiles home
//   field hands     on the wheat, iron and horse tiles of a holding that
//                   harvested in the last WORK_BELLS bells
//   townsfolk       by tier, fewer the longer the lord has been away
// Nothing is invented: a holding whose lord has done nothing lately is quiet.

import { hostParts } from '../faddr.mjs';

export const LORD_BELLS = 2;
export const WORK_BELLS = 6;
export const QUIET_BELLS = 24;
export const LIFE_MAX = 6000;

const keyOf = (p, q, site) => `${p},${q},${site}`;
const holdingOfRecord = r => {
  if (r.host_id !== undefined) { const h = hostParts(r.host_id); return h ? keyOf(h.p, h.q, h.site) : null; }
  return r.p !== undefined && r.site !== undefined ? keyOf(Number(r.p), Number(r.q), Number(r.site)) : null;
};

/**
 * Fold chronicle records (`{record}` or bare records) into a life map:
 * "P,Q,site" → `{last, harvest, build: {item, doneAt}|null, train: {unit, n, doneAt}|null, muster, depart, explore}`
 * (bells; done-at in chain seconds). Returns the same map (mutated), at most LIFE_MAX holdings.
 */
export function updateLife(life, records) {
  for (const x of records ?? []) {
    const r = x.record ?? x;
    const kinds = { HARVEST: 'harvest', BUILD: 'build', TRAIN: 'train', MUSTER: 'muster', DEPART: 'depart', EXPLORE: 'explore', SETTLE: 'settle' };
    const what = kinds[r.name];
    if (!what) continue;
    const k = holdingOfRecord(r);
    if (!k) continue;
    const bell = Number(r.bell);
    const rec = life.get(k) ?? { last: -1, harvest: -1, build: null, train: null, muster: -1, depart: -1, explore: -1, settle: -1 };
    rec.last = Math.max(rec.last, bell);
    if (what === 'build') rec.build = { item: Number(r.item), doneAt: Number(r.done_at), bell };
    else if (what === 'train') rec.train = { unit: Number(r.unit), n: Number(r.n), doneAt: Number(r.done_at), bell };
    else rec[what] = Math.max(rec[what], bell);
    life.delete(k); life.set(k, rec);
  }
  while (life.size > LIFE_MAX) life.delete(life.keys().next().value);
  return life;
}

/**
 * What a holding is visibly doing at (bell, chain seconds): `{lord, doing,
 * building, training, harvesting, working, liveliness}` — `doing` is the
 * lord's pose ('build' | 'train' | 'harvest' | 'march' | 'muster' | 'explore'
 * | null), `liveliness` 0..1 scales the townsfolk.
 */
export function lifeAt(rec, bell, now) {
  if (!rec) return { lord: false, doing: null, building: false, training: false, harvesting: false, working: false, liveliness: 0.35 };
  const since = b => b >= 0 && bell - b <= LORD_BELLS;
  const building = !!rec.build && rec.build.doneAt > now;
  const training = !!rec.train && rec.train.doneAt > now;
  const harvesting = rec.harvest >= 0 && bell - rec.harvest <= 1;
  const doing = since(rec.depart) ? 'march' : since(rec.muster) ? 'muster' : building && since(rec.build.bell) ? 'build' : training && since(rec.train.bell) ? 'train'
    : since(rec.harvest) ? 'harvest' : since(rec.explore) ? 'explore' : since(rec.settle) ? 'settle' : null;
  const away = rec.last < 0 ? Infinity : bell - rec.last;
  return {
    lord: away <= LORD_BELLS, doing, building, training, harvesting,
    working: rec.harvest >= 0 && bell - rec.harvest <= WORK_BELLS,
    liveliness: away <= LORD_BELLS ? 1 : away <= QUIET_BELLS / 2 ? 0.7 : away <= QUIET_BELLS ? 0.45 : 0.25,
  };
}

/** The hover tip's line for a lord who is out: "領主 Kaito が建設を見守っている". */
export function lordLine(name, doing, L) {
  switch (doing) {
    case 'build': return L`領主 ${name} が建設を見守っている`;
    case 'train': return L`領主 ${name} が兵を訓練している`;
    case 'harvest': return L`領主 ${name} が収穫を取り入れている`;
    case 'march': return L`領主 ${name} が出陣を率いている`;
    case 'muster': return L`領主 ${name} が軍勢を編成している`;
    case 'explore': return L`領主 ${name} が探索を指揮している`;
    case 'settle': return L`領主 ${name} がこの地に着いたばかり`;
    default: return L`領主 ${name} が領地にいる`;
  }
}
