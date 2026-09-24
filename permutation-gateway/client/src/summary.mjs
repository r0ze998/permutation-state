// A compact, token-cheap summary of a fogged view for language models.
// Everything in it comes from `GET /api/state?civ=N`; nothing is invented.

const hexDist = (a, b) => (Math.abs(a.q - b.q) + Math.abs(a.r - b.r) + Math.abs(a.q + a.r - b.q - b.r)) / 2;

/**
 * @param {object} v  the view from GameClient.state()
 * @param {object} [o]
 * @param {number} [o.near] radius around your cities/units for foreign things
 */
export function summarize(v, { near = 6 } = {}) {
  const me = v.me;
  const mine = v.units.filter(u => u.owner === me);
  const myCities = v.cities.filter(c => c.owner === me);
  const anchors = [...mine, ...myCities];
  const close = x => anchors.some(a => hexDist(a, x) <= near);
  const e = v.economy;
  return {
    tick: v.tick, ticks: v.ticks, secondsLeft: Math.round(v.secondsLeft), you: me, name: v.civs[me]?.name,
    orders: { spendable: e.budget + e.bank, budget: e.budget, bank: e.bank, note: 'each order costs 1 (ExchangeOrder and RevealRationale cost 0)' },
    economy: { gold: e.gold, goldIncome: e.goldIncome, iron: e.iron, horses: e.horses, influence: e.influence, science: e.scienceStore,
      research: e.research, researchQueue: e.researchQueue, techs: e.techs, warWeariness: e.warWeariness },
    cities: myCities.map(c => ({ id: c.id, q: c.q, r: c.r, pop: c.pop, capital: c.capital, focus: c.focus, queue: c.queue, buildings: c.buildings,
      defense: `${c.defense}/${c.defenseMax}`, loyalty: c.loyalty })),
    units: mine.map(u => ({ id: u.id, type: u.type, q: u.q, r: u.r, troops: u.troops, path: u.path?.length ? u.path : undefined, standing: u.standing ?? undefined })),
    civs: v.civs.filter(c => c.id !== me).map(c => ({ id: c.id, name: c.name, relation: c.relation, cities: c.cities, troopsSeen: c.troopsSeen,
      dominion: c.dominion, concord: c.concord, starGate: c.stages, aggressor: c.aggressor || undefined, truceUntil: c.truceUntil || undefined })),
    nearby: {
      foreignUnits: v.units.filter(u => u.owner !== me && close(u)).map(u => ({ id: u.id, owner: u.owner, type: u.type, q: u.q, r: u.r, troops: u.troops })),
      foreignCities: v.cities.filter(c => c.owner !== me && close(c)).map(c => ({ id: c.id, owner: c.owner, q: c.q, r: c.r, pop: c.pop, defense: c.defense, seenTick: c.seenTick ?? undefined })),
      cityStates: v.cityStates.filter(close).map(cs => ({ id: cs.id, q: cs.q, r: cs.r, specialty: cs.specialty, suzerain: cs.suzerain, myInfluence: cs.myInfluence })),
    },
    proposals: v.proposals.filter(p => p.to === me),
    lastTick: (v.lastSummary || []).map(l => l.split('|')[1] ?? l),
    fog: { unexplored: [...(v.fog || '')].filter(c => c === '0').length, tiles: (v.fog || '').length },
  };
}
