// A compact, token-cheap summary of a nation's fogged view for language
// models. Everything in it comes from `GET /api/state?member=M`; nothing is
// invented.

import { hexDist } from './hexgrid.mjs';

/**
 * @param {object} v  the view from GameClient.state()
 * @param {object} [o]
 * @param {number} [o.near]   radius around your cities/units for foreign things
 * @param {number} [o.member] your member id
 */
export function summarize(v, { near = 6, member = null } = {}) {
  const civ = v.me;
  const mine = v.units.filter(u => u.owner === civ);
  const myCities = v.cities.filter(c => c.owner === civ);
  const anchors = [...mine, ...myCities];
  const close = x => anchors.some(a => hexDist(a, x) <= near);
  const e = v.economy;
  const gov = v.gov ?? {};
  const held = (gov.offices ?? []).filter(o => o.holder?.id === member).map(o => o.role);
  const ach = v.achievements?.nations?.[civ];
  const me = member === null ? null : (v.members ?? []).find(m => m.id === member);
  return {
    tick: v.tick, ticks: v.ticks, secondsLeft: Math.round(v.secondsLeft), nation: v.civs[civ]?.name, civ,
    you: { member, name: me?.name, officesHeld: held, note: held.length ? 'you order only within your offices; propose the rest' : 'you hold no office: propose orders, support, vote, recall' },
    offices: (gov.offices ?? []).map(o => ({ role: o.role, holder: o.holder ? `${o.holder.name} (#${o.holder.id}, ${o.holder.kind})` : 'acting official (bot)', active: o.active,
      spendable: e?.offices?.find(x => x.role === o.role)?.spendable })),
    election: { next: gov.nextElection, voteOpen: gov.voteOpen, candidates: (gov.candidates ?? []).map(c => ({ role: c.role, candidates: c.candidates.map(x => ({ id: x.member?.id, name: x.member?.name, votes: x.votes })) })) },
    proposals: (gov.proposals ?? []).slice(0, 6).map(p => ({ id: p.id, role: p.role, by: p.proposer?.name, supporters: p.supporters, orders: p.orders })),
    recalls: gov.recalls ?? [],
    achievements: ach ? { tiers: { hegemony: ach.tiers[0], prosperity: ach.tiers[1], science: ach.tiers[2], concord: ach.tiers[3] }, era: ach.era, points: ach.points } : null,
    projection: { nationShare: v.projection?.nationShare?.[civ], yourPayout: member !== null ? v.projection?.perMember?.[member] : undefined, pool: v.projection?.pool },
    economy: e && { gold: e.gold, goldIncome: e.goldIncome, iron: e.iron, horses: e.horses, influence: e.influence, science: e.scienceStore,
      research: e.research, researchQueue: e.researchQueue, techs: e.techs, warWeariness: e.warWeariness, treasuryUsdc: e.treasury, tariffBps: e.tariffBps },
    cities: myCities.map(c => ({ id: c.id, q: c.q, r: c.r, pop: c.pop, capital: c.capital, focus: c.focus, queue: c.queue, buildings: c.buildings,
      defense: `${c.defense}/${c.defenseMax}`, loyalty: c.loyalty })),
    units: mine.map(u => ({ id: u.id, type: u.type, q: u.q, r: u.r, troops: u.troops, path: u.path?.length ? u.path : undefined, standing: u.standing ?? undefined })),
    nations: v.civs.filter(c => c.id !== civ).map(c => ({ id: c.id, name: c.name, relation: c.relation, cities: c.cities, troopsSeen: c.troopsSeen,
      era: c.era, tiers: c.tiers, starGate: c.stages, aggressor: c.aggressor || undefined, truceUntil: c.truceUntil || undefined })),
    nearby: {
      foreignUnits: v.units.filter(u => u.owner !== civ && close(u)).map(u => ({ id: u.id, owner: u.owner, type: u.type, q: u.q, r: u.r, troops: u.troops })),
      foreignCities: v.cities.filter(c => c.owner !== civ && close(c)).map(c => ({ id: c.id, owner: c.owner, q: c.q, r: c.r, pop: c.pop, defense: c.defense, seenTick: c.seenTick ?? undefined })),
      cityStates: v.cityStates.filter(close).map(cs => ({ id: cs.id, q: cs.q, r: cs.r, specialty: cs.specialty, suzerain: cs.suzerain, myInfluence: cs.myInfluence })),
    },
    treatyOffers: v.proposals.filter(p => p.to === civ),
    skippedLastTick: v.skipped ?? [],
    lastTick: (v.lastSummary || []).map(l => l.split('|')[1] ?? l),
    fog: { unexplored: [...(v.fog || '')].filter(c => c === '0').length, tiles: (v.fog || '').length },
  };
}
