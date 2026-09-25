#!/usr/bin/env node
// Reference agent 1: rule-based, no model. A member of one nation: it sees
// only the view every member has (the whole world), uses the same previews a human sees, signs
// its own batches for the offices it holds, and proposes the rest.
//
//   node agents/rule-agent.mjs --name Gaia --server http://127.0.0.1:4185 --gateway http://127.0.0.1:4191 [--civ 5] [--stand Science,Diplomat]
//
// Priorities each tick: research → found cities → expand (settlers) →
// scout → take favourable fights → keep every city building →
// accept peace. The rationale names what it did and why, and is revealed
// after the tick.
import { parseArgs, runAgent } from './runner.mjs';
import { hexDist as dist } from '../client/src/hexgrid.mjs';
import { officeOf } from '../client/src/offices.mjs';

const TECH_PLAN = ['Agriculture', 'BronzeWorking', 'Writing', 'Masonry', 'Mysticism', 'Currency', 'Archery', 'Mathematics', 'Philosophy',
  'IronWorking', 'HorsebackRiding', 'Engineering', 'Astronomy', 'Physics', 'CelestialMechanics', 'Chivalry'];
const BUILD_PLAN = ['Granary', 'Workshop', 'Academy', 'Market', 'Temple', 'Walls', 'StarGate1', 'StarGate2', 'StarGate3', 'Barracks'];
const TROOPS = ['Knight', 'Crossbowman', 'Pikeman', 'Horseman', 'Archer', 'Spearman'];
const GOOD_GROUND = { Grassland: 3, Plains: 3, Hills: 2, Forest: 1 };
const JA = { Agriculture: '農業', BronzeWorking: '青銅器', Archery: '弓術', HorsebackRiding: '騎乗', Masonry: '石工', Mysticism: '神秘主義', Writing: '筆記', Currency: '通貨',
  IronWorking: '製鉄', Mathematics: '数学', Chivalry: '騎士道', Philosophy: '哲学', Engineering: '工学', Astronomy: '天文学', Physics: '物理学', CelestialMechanics: '天体力学',
  Granary: '穀物庫', Workshop: '工房', Temple: '神殿', Market: '市場', Academy: '学術院', Barracks: '兵舎', Walls: '城壁', StarGate1: 'スターゲートI', StarGate2: 'スターゲートII', StarGate3: 'スターゲートIII',
  Spearman: '槍兵', Archer: '弓兵', Horseman: '騎兵', Pikeman: '長槍兵', Crossbowman: '弩兵', Knight: '騎士', Scout: '斥候', Settler: '開拓者' };
const ja = k => JA[k] || k;

async function decide({ game, view: v, map, held }) {
  const me = v.me;
  const tiles = map.tiles.map(([q, r, terrain, river, resource], i) => ({ q, r, terrain, river, resource, i }));
  const tileAt = new Map(tiles.map(t => [`${t.q},${t.r}`, t]));
  const fogAt = t => v.fog[t.i];
  const myCities = v.cities.filter(c => c.owner === me);
  const myUnits = v.units.filter(u => u.owner === me);
  const cap = myCities.find(c => c.capital) || myCities[0];
  const wish = []; // [priority, order, note]
  const add = (pri, order, note) => wish.push({ pri, order, note });

  // Research: keep up to three techs queued, in plan order.
  if (!v.economy.researchQueue.length) {
    const opts = await game.preview('research');
    const held = new Set(v.economy.techs);
    const pick = [];
    for (const t of TECH_PLAN) {
      const o = opts.find(x => x.tech === t);
      if (!o || o.held || pick.length >= 3) continue;
      if (o.prereqs.every(p => held.has(p) || pick.includes(p))) pick.push(t);
    }
    if (pick.length) add(90, { type: 'SetResearch', techs: pick }, `研究: ${pick.map(ja).join('→')}`);
  }

  // Diplomacy: accept peace and non-aggression; never start wars.
  for (const p of v.proposals.filter(p => p.to === me)) {
    if (p.kind === 'Peace') add(95, { type: 'AcceptPeace', civ: p.from }, `${v.civs[p.from].name}との講和を受諾`);
    if (p.kind === 'Nap' && v.economy.gold >= p.bond) add(70, { type: 'AcceptNap', civ: p.from, bond: p.bond }, `${v.civs[p.from].name}と不可侵`);
  }

  // Treasury contracts (V5 §18.6): a peaceful nation takes money to keep the
  // peace, and pays others to check the leader: once, from tick 50, an open
  // bounty on the leading nation's capital.
  for (const c of (v.contracts || []).filter(c => c.to === me && c.accepted == null && c.offered < v.tick)) {
    if ((c.term.kind === 'Peace' || c.term.kind === 'KeepNap') && c.total >= 1_000_000) {
      add(96, { type: 'AcceptContract', id: c.id }, `${v.civs[c.from].name}の契約（${(c.total / 1e6).toFixed(1)} USDC）を受諾`);
    }
  }
  const usdcFree = (v.economy.treasury ?? 0) - (v.economy.contractIncome ?? 0);
  const posted = (v.contracts || []).some(c => c.from === me && c.term.kind === 'Capture');
  if (v.tick >= 50 && v.tick < 120 && usdcFree >= 1_000_000 && !posted && !game.bountyPosted) {
    const leader = v.civs.filter(c => c.id !== me).sort((a, b) => (b.points ?? 0) - (a.points ?? 0) || b.cities - a.cities)[0];
    const capital = leader && v.cities.find(c => c.owner === leader.id && c.capital);
    if (capital) {
      game.bountyPosted = true;
      add(60, { type: 'OfferContract', term: { kind: 'Capture', city: capital.id }, usdc: 1_000_000, deadline: Math.min(v.tick + 60, v.ticks - 1) },
        `首位の${leader.name}の首都（都市${capital.id}）に 1 USDC の懸賞`);
    }
  }

  // Units.
  const knownCities = [...v.cities, ...v.cityStates];
  for (const u of myUnits) {
    if (u.path?.length) continue; // still walking an earlier order
    const p = await game.preview('unit', { id: u.id });
    if (u.type === 'Settler') {
      if (p.canFound) { add(100, { type: 'FoundCity', settler: u.id }, `開拓者${u.id}が(${u.q},${u.r})に都市を建設`); continue; }
      // The best reachable site: open ground, 3+ hexes from every known city, near home.
      let best = null;
      for (const [q, r, ticks] of p.reach) {
        const t = tileAt.get(`${q},${r}`);
        if (!t || !GOOD_GROUND[t.terrain] || ticks > 5 || fogAt(t) === '0') continue;
        if (knownCities.some(c => dist(c, t) < 3)) continue;
        const score = GOOD_GROUND[t.terrain] + (t.river ? 1 : 0) + (t.resource ? 2 : 0) - ticks * 0.6 - (cap ? Math.abs(dist(cap, t) - 4) * 0.5 : 0);
        if (!best || score > best.score) best = { q, r, score };
      }
      if (best) {
        const path = await game.preview('path', { unit: u.id, q: best.q, r: best.r });
        if (path.path) add(85, { type: 'MoveUnit', unit: u.id, path: path.path }, `開拓者${u.id}を建設地(${best.q},${best.r})へ`);
      }
      continue;
    }
    // Fights worth taking: a civilian to capture, or clearly more damage dealt than taken.
    const fight = p.attacks.filter(a => !a.blocked && a.forecast && (a.forecast.captureCivilian || a.forecast.toDefender >= 1.3 * a.forecast.toAttacker))
      .sort((a, b) => (b.forecast.toDefender ?? 99) - (a.forecast.toDefender ?? 99))[0];
    if (fight) { add(88, { type: 'Attack', army: u.id, target: fight.target }, `${ja(u.type)}${u.id}が(${fight.q},${fight.r})を攻撃（有利な見込み）`); continue; }
    if (u.type === 'Scout') {
      // Toward the most unexplored ground within a few ticks.
      let best = null;
      for (const [q, r, ticks] of p.reach) {
        if (ticks > 3) continue;
        const unseen = tiles.filter(t => fogAt(t) === '0' && dist(t, { q, r }) <= 3).length;
        const score = unseen - ticks * 0.5;
        if (unseen && (!best || score > best.score)) best = { q, r, score, unseen };
      }
      if (best) {
        const path = await game.preview('path', { unit: u.id, q: best.q, r: best.r });
        if (path.path) add(60, { type: 'MoveUnit', unit: u.id, path: path.path }, `斥候${u.id}で未踏の${best.unseen}マスへ`);
      }
      continue;
    }
    // Soldiers far from home walk back to guard the capital.
    if (cap && dist(u, cap) > 3) {
      const path = await game.preview('path', { unit: u.id, q: cap.q, r: cap.r });
      if (path.path) add(40, { type: 'MoveUnit', unit: u.id, path: path.path.slice(0, 6) }, `${ja(u.type)}${u.id}を首都の守りへ`);
    }
  }

  // Cities: never idle.
  const soldiers = myUnits.filter(u => !u.civilian).length;
  const settlers = myUnits.filter(u => u.type === 'Settler').length;
  for (const c of myCities) {
    if (c.queue?.length) continue;
    const { options } = await game.preview('city', { id: c.id });
    const ok = options.filter(o => !o.blocked);
    const find = pred => ok.find(o => pred(o.item));
    let pick = null, why = '';
    if (myCities.length + settlers < 4 && c.pop >= 2 && (pick = find(i => i.kind === 'Settler'))) why = '拡張のため';
    else if (soldiers < myCities.length + 1 && (pick = TROOPS.map(t => find(i => i.kind === 'Troops' && i.unit === t)).find(Boolean))) why = '守りが手薄';
    else if ((pick = BUILD_PLAN.map(b => find(i => i.kind === 'Building' && i.building === b)).find(Boolean))) why = '内政';
    else if ((pick = find(i => i.kind === 'Scout'))) why = '探索';
    if (pick) add(80, { type: 'SetQueue', city: c.id, items: [pick.item] }, `都市${c.id}: ${ja(pick.item.building || pick.item.unit || pick.item.kind)}（${why}）`);
  }

  // Spend each office's budget on its most important orders; for offices
  // we do not hold, keep the best two as proposals.
  const spendable = Object.fromEntries((v.economy.offices || []).map(o => [o.role, o.spendable]));
  const used = {};
  const chosen = [];
  for (const w of wish.sort((a, b) => b.pri - a.pri)) {
    const role = officeOf(w.order, v);
    const cap = held.includes(role) ? spendable[role] ?? 0 : 2;
    if ((used[role] ?? 0) >= cap) continue;
    used[role] = (used[role] ?? 0) + 1;
    chosen.push(w);
  }
  const check = await game.validate(chosen.map(w => w.order));
  const skipped = new Set((check.warnings || []).map(w => w.index));
  const final = chosen.filter((_, i) => !skipped.has(i));
  const notes = final.filter(w => held.includes(officeOf(w.order, v))).map(w => w.note);
  const rationale = notes.length ? `${notes.join(' / ')}（${held.join('・')}として）` : '担当の命令で急ぐものはありません。枠を繰り越します。';
  return { orders: final.map(w => w.order), rationale };
}

const args = parseArgs(process.argv, { name: 'Gaia' });
runAgent({ name: args.name, policy: 'rule-agent/v2', decide, args }).catch(e => { console.error(e); process.exit(1); });
