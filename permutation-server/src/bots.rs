//! Scripted rule-based bots with four personalities (Warlord, Builder,
//! Diplomat, Scholar). They play through the same public order API as a
//! human and read the full state (there is no fog-of-war view yet), so they
//! are a test harness and sparring partner, not the reference agent.

use permutation_rules::buildings::Building;
use permutation_rules::hex::Hex;
use permutation_rules::orders::{AttackTarget, Order};
use permutation_rules::state::{CivId, Owner, ProposalKind, QueueItem, Relation, WorldState};
use permutation_rules::tech::{Tech, TECHS};
use permutation_rules::tick::may_enter;
use permutation_rules::units::{stats, UnitClass, UnitType};
use permutation_rules::Ruleset;
use std::collections::{BTreeSet, HashMap, VecDeque};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Persona {
    Warlord,
    Builder,
    Diplomat,
    Scholar,
}

impl Persona {
    pub fn name(self) -> &'static str {
        match self {
            Persona::Warlord => "Warlord",
            Persona::Builder => "Builder",
            Persona::Diplomat => "Diplomat",
            Persona::Scholar => "Scholar",
        }
    }
    fn target_cities(self) -> usize {
        match self {
            Persona::Warlord => 4,
            Persona::Builder => 6,
            Persona::Diplomat => 5,
            Persona::Scholar => 4,
        }
    }
    fn research(self) -> &'static [Tech] {
        use Tech::*;
        match self {
            Persona::Warlord => &[
                BronzeWorking,
                Archery,
                HorsebackRiding,
                IronWorking,
                Agriculture,
                Masonry,
                Chivalry,
                Currency,
                Writing,
                Mathematics,
                Mysticism,
                Philosophy,
                Engineering,
            ],
            Persona::Builder => &[
                Agriculture,
                BronzeWorking,
                Currency,
                Writing,
                Masonry,
                Mysticism,
                Mathematics,
                IronWorking,
                Philosophy,
                Engineering,
                Astronomy,
                Physics,
                CelestialMechanics,
            ],
            Persona::Diplomat => &[
                Agriculture,
                Mysticism,
                Writing,
                BronzeWorking,
                Currency,
                Philosophy,
                Masonry,
                Archery,
                Mathematics,
                Astronomy,
                IronWorking,
                Physics,
                CelestialMechanics,
            ],
            Persona::Scholar => &[
                Agriculture,
                Writing,
                BronzeWorking,
                Currency,
                Mysticism,
                Mathematics,
                Philosophy,
                Astronomy,
                Physics,
                CelestialMechanics,
                Archery,
                Masonry,
                IronWorking,
            ],
        }
    }
    fn buildings(self) -> &'static [Building] {
        use Building::*;
        match self {
            Persona::Warlord => &[Barracks, Granary, Workshop, Walls, Market, Temple, Academy],
            Persona::Builder => &[Granary, Workshop, Market, Academy, Temple, Walls, Barracks],
            Persona::Diplomat => &[Granary, Temple, Workshop, Academy, Market, Walls, Barracks],
            Persona::Scholar => &[Granary, Academy, Workshop, Temple, Market, Walls, Barracks],
        }
    }
}

pub const NAMES: [&str; 6] = ["Aster", "Borealis", "Cinder", "Dunmar", "Ember", "Fjordhal"];
pub const PERSONAS: [Persona; 6] = [
    Persona::Warlord,
    Persona::Builder,
    Persona::Diplomat,
    Persona::Scholar,
    Persona::Warlord,
    Persona::Diplomat,
];

/// A scripted rule-based player. Reads the full state (no fog view yet).
pub struct Bot {
    pub civ: CivId,
    pub persona: Persona,
    pub war_started: HashMap<CivId, u16>,
}

impl Bot {
    pub fn new(civ: CivId, persona: Persona) -> Self {
        Bot {
            civ,
            persona,
            war_started: HashMap::new(),
        }
    }
}

// ------------------------------------------------------------------ helpers

fn passable(s: &WorldState, h: Hex) -> bool {
    s.map.tile(h).is_some_and(|t| t.terrain.is_passable())
}

/// BFS over tiles the civ may enter; returns the path excluding `from`.
fn path_to(
    s: &WorldState,
    r: &Ruleset,
    civ: CivId,
    from: Hex,
    goal: impl Fn(Hex) -> bool,
) -> Option<Vec<Hex>> {
    let mut prev: HashMap<Hex, Hex> = HashMap::new();
    let mut seen: BTreeSet<Hex> = BTreeSet::new();
    let mut q = VecDeque::new();
    q.push_back(from);
    seen.insert(from);
    while let Some(h) = q.pop_front() {
        if h != from && goal(h) {
            let mut path = vec![h];
            let mut cur = h;
            while let Some(p) = prev.get(&cur) {
                if *p == from {
                    break;
                }
                path.push(*p);
                cur = *p;
            }
            path.reverse();
            path.truncate(r.max_path_len as usize);
            return Some(path);
        }
        if seen.len() > 900 {
            break;
        }
        for n in h.neighbors() {
            if seen.contains(&n) || !passable(s, n) || !may_enter(s, r, Some(civ), n) {
                continue;
            }
            seen.insert(n);
            prev.insert(n, h);
            q.push_back(n);
        }
    }
    None
}

pub fn city_count(s: &WorldState, civ: CivId) -> usize {
    s.living_cities_of(civ).count()
}

fn my_units(s: &WorldState, civ: CivId) -> impl Iterator<Item = &permutation_rules::state::Unit> {
    s.units
        .iter()
        .filter(move |u| u.alive && u.owner == Owner::Civ(civ))
}

pub fn troops_of(s: &WorldState, civ: CivId) -> u32 {
    my_units(s, civ)
        .filter(|u| !u.unit_type.is_civilian())
        .map(|u| u.troops / 1000)
        .sum()
}

fn good_site(s: &WorldState, r: &Ruleset, civ: CivId, h: Hex) -> bool {
    let min = r.city_min_distance as u32;
    let Some(t) = s.map.tile(h) else { return false };
    t.terrain.is_passable()
        && t.owner_city
            .and_then(|c| s.cities.get(c as usize))
            .is_none_or(|c| !c.alive || c.owner == Some(civ))
        && s.cities
            .iter()
            .all(|c| !c.alive || c.hex.distance(h) >= min)
        && s.city_states.iter().all(|c| c.hex.distance(h) >= min)
        && may_enter(s, r, Some(civ), h)
}

fn site_value(s: &WorldState, h: Hex) -> u32 {
    s.map
        .tiles
        .iter()
        .filter(|t| t.hex.distance(h) <= 2)
        .map(|t| {
            let (f, p, g) = t.yields();
            2 * f + 2 * p + g + if t.resource.is_some() { 3 } else { 0 }
        })
        .sum()
}

// ------------------------------------------------------------------ the bot

impl Bot {
    pub fn orders(&mut self, s: &WorldState, r: &Ruleset) -> Vec<Order> {
        let civ = self.civ;
        let me = &s.civs[civ as usize];
        let n = s.civs.len() as CivId;
        let budget = (me.tick_budget + me.order_bank) as usize;
        // (priority, order): lower number = more important.
        let mut wish: Vec<(u8, Order)> = Vec::new();

        // ---- diplomacy: answer proposals addressed to me
        for p in s
            .proposals
            .iter()
            .filter(|p| p.to == civ && p.tick < s.tick)
        {
            let accept = match (p.kind, self.persona) {
                (ProposalKind::Peace, Persona::Warlord) => troops_of(s, civ) < troops_of(s, p.from),
                (ProposalKind::Peace, _) => true,
                (ProposalKind::Nap { .. }, Persona::Warlord) => false,
                (ProposalKind::Nap { .. }, _) => me.gold >= 40_000,
                (ProposalKind::Alliance, Persona::Diplomat | Persona::Builder) => true,
                (ProposalKind::Alliance, _) => false,
            };
            if accept {
                let o = match p.kind {
                    ProposalKind::Peace => Order::AcceptPeace { civ: p.from },
                    ProposalKind::Nap { .. } => Order::AcceptNap {
                        civ: p.from,
                        bond: 30,
                    },
                    ProposalKind::Alliance => Order::AcceptAlliance { civ: p.from },
                };
                wish.push((0, o));
            }
        }

        // ---- diplomacy: my own initiatives
        let my_cap = me.capital.map(|c| s.cities[c as usize].hex);
        let nearest_civ = |filter: &dyn Fn(CivId) -> bool| -> Option<CivId> {
            let cap = my_cap?;
            (0..n)
                .filter(|o| *o != civ && filter(*o))
                .filter_map(|o| {
                    let oc = s.civs[o as usize].capital?;
                    Some((s.cities[oc as usize].hex.distance(cap), o))
                })
                .min()
                .map(|(_, o)| o)
        };
        match self.persona {
            Persona::Warlord => {
                let at_war = (0..n).any(|o| s.at_war(civ, o));
                if !at_war && s.tick >= 30 && troops_of(s, civ) >= 12 {
                    if let Some(t) = nearest_civ(&|o| matches!(s.relation(civ, o), Relation::Peace))
                    {
                        wish.push((1, Order::DeclareWar { civ: t }));
                        self.war_started.insert(t, s.tick);
                    }
                }
                for o in 0..n {
                    let since = self.war_started.get(&o).copied().unwrap_or(s.tick);
                    if s.at_war(civ, o) && (s.tick - since >= 45 || troops_of(s, civ) < 4) {
                        wish.push((1, Order::ProposePeace { civ: o }));
                    }
                }
            }
            Persona::Diplomat => {
                if s.tick == 6 || s.tick == 40 {
                    for o in 0..n {
                        if o != civ && PERSONAS[o as usize] == Persona::Diplomat {
                            wish.push((1, Order::ProposeAlliance { civ: o }));
                        }
                    }
                }
                if me.influence >= 15_000 {
                    if let Some(cap) = my_cap {
                        if let Some(cs) = s
                            .city_states
                            .iter()
                            .filter(|cs| cs.captured_by.is_none())
                            .min_by_key(|cs| cs.hex.distance(cap))
                        {
                            let amount = (me.influence / 1000) as u32;
                            wish.push((
                                3,
                                Order::SendEnvoy {
                                    city_state: cs.id,
                                    influence: amount,
                                },
                            ));
                        }
                    }
                }
            }
            Persona::Builder | Persona::Scholar => {
                if s.tick % 20 == 10 && me.gold >= 60_000 {
                    if let Some(t) = nearest_civ(&|o| matches!(s.relation(civ, o), Relation::Peace))
                    {
                        wish.push((2, Order::ProposeNap { civ: t, bond: 30 }));
                    }
                }
            }
        }
        for o in 0..n {
            if s.at_war(civ, o) && self.persona != Persona::Warlord {
                wish.push((1, Order::ProposePeace { civ: o }));
            }
        }

        // ---- research
        if me.research_queue.is_empty() {
            let mut plan = me.techs;
            let mut q = Vec::new();
            let order = self
                .persona
                .research()
                .iter()
                .chain(TECHS.iter().map(|t| &t.tech));
            for t in order {
                if q.len() == 3 {
                    break;
                }
                if !plan.has(*t) && plan.prereqs_met(*t) {
                    plan.insert(*t);
                    q.push(*t);
                }
            }
            if !q.is_empty() {
                wish.push((2, Order::SetResearch { techs: q }));
            }
        }

        // ---- cities
        let settlers_alive = my_units(s, civ)
            .filter(|u| u.unit_type == UnitType::Settler)
            .count();
        let queued_settlers = s
            .living_cities_of(civ)
            .filter(|c| c.queue.first() == Some(&QueueItem::Settler))
            .count();
        let mut want_settlers = self
            .persona
            .target_cities()
            .saturating_sub(city_count(s, civ) + settlers_alive + queued_settlers);
        let troop_cap = if self.persona == Persona::Warlord {
            40
        } else {
            16
        };
        let army = troops_of(s, civ);
        let has = |t: Tech| me.techs.has(t);
        for c in s.living_cities_of(civ) {
            let buildable: Vec<Building> = self
                .persona
                .buildings()
                .iter()
                .copied()
                .filter(|b| {
                    !c.buildings.has(*b)
                        && permutation_rules::buildings::info(*b).tech.is_none_or(has)
                        && !c.queue.contains(&QueueItem::Building(*b))
                })
                .collect();
            let star = if matches!(self.persona, Persona::Scholar | Persona::Builder)
                && me.capital == Some(c.id)
            {
                let stage = c.buildings.star_gate_stages() as usize;
                [
                    (Building::StarGate1, Tech::Astronomy),
                    (Building::StarGate2, Tech::Physics),
                    (Building::StarGate3, Tech::CelestialMechanics),
                ]
                .get(stage)
                .filter(|(b, t)| has(*t) && !c.queue.contains(&QueueItem::Building(*b)))
                .map(|(b, _)| *b)
            } else {
                None
            };
            let settle = want_settlers > 0 && c.pop >= 2;
            // CityQueueRepeat keeps re-queuing the last unit, so a queue that
            // only repeats a unit is re-planned whenever something better exists.
            let repeating = c.queue.len() == 1
                && matches!(c.queue[0], QueueItem::Troops { .. } | QueueItem::Settler);
            let needs = c.queue.is_empty()
                || (repeating
                    && (settle
                        || star.is_some()
                        || !buildable.is_empty()
                        || army >= troop_cap
                        || (c.queue[0] == QueueItem::Settler && want_settlers == 0)));
            if !needs {
                continue;
            }
            let mut items = Vec::new();
            if let Some(b) = star {
                items.push(QueueItem::Building(b));
            }
            if settle {
                items.push(QueueItem::Settler);
                want_settlers -= 1;
            }
            for b in &buildable {
                if items.len() >= 2 {
                    break;
                }
                items.push(QueueItem::Building(*b));
            }
            if army < troop_cap && items.len() < 3 {
                let unit = if has(Tech::IronWorking) && me.iron >= 5_000 {
                    UnitType::Pikeman
                } else if self.persona == Persona::Warlord
                    && has(Tech::HorsebackRiding)
                    && s.tick % 2 == 0
                {
                    UnitType::Horseman
                } else if has(Tech::Archery) && s.tick % 3 == 0 {
                    UnitType::Archer
                } else {
                    UnitType::Spearman
                };
                let n = if self.persona == Persona::Warlord {
                    8
                } else {
                    5
                };
                items.push(QueueItem::Troops { unit, n });
            }
            items.truncate(3);
            if items != c.queue {
                wish.push((2, Order::SetQueue { city: c.id, items }));
            }
        }
        if matches!(self.persona, Persona::Builder | Persona::Scholar) && me.gold > 150_000 {
            if let Some(cap) = me.capital {
                wish.push((
                    4,
                    Order::Purchase {
                        city: cap,
                        gold: 60,
                    },
                ));
            }
        }

        // ---- settlers
        let mut claimed: Vec<Hex> = Vec::new();
        for u in my_units(s, civ).filter(|u| u.unit_type == UnitType::Settler) {
            if good_site(s, r, civ, u.hex)
                && s.living_cities_of(civ).all(|c| c.hex.distance(u.hex) <= 7)
            {
                wish.push((1, Order::FoundCity { settler: u.id }));
                continue;
            }
            if !u.path.is_empty() {
                continue;
            }
            let target = s
                .map
                .tiles
                .iter()
                .filter(|t| {
                    let near_mine = s
                        .living_cities_of(civ)
                        .map(|c| c.hex.distance(t.hex))
                        .min()
                        .unwrap_or(99);
                    (3..=6).contains(&near_mine)
                        && !claimed.iter().any(|c| c.distance(t.hex) < 3)
                        && good_site(s, r, civ, t.hex)
                })
                .max_by_key(|t| {
                    (
                        site_value(s, t.hex),
                        std::cmp::Reverse(t.hex.distance(u.hex)),
                    )
                })
                .map(|t| t.hex);
            if let Some(goal) = target {
                claimed.push(goal);
                if let Some(path) = path_to(s, r, civ, u.hex, |h| h == goal) {
                    wish.push((2, Order::MoveUnit { unit: u.id, path }));
                }
            }
        }

        // ---- armies
        let enemies: Vec<CivId> = (0..n).filter(|o| s.at_war(civ, *o)).collect();
        let garrison = my_cap.and_then(|cap| {
            my_units(s, civ)
                .find(|u| u.hex == cap && !u.unit_type.is_civilian())
                .map(|u| u.id)
        });
        for u in my_units(s, civ).filter(|u| !u.unit_type.is_civilian()) {
            let st = stats(u.unit_type);
            let reach = if st.class == UnitClass::Ranged {
                st.range as u32
            } else {
                1
            };
            // Attack anything hostile in reach: cities first, then the weakest army.
            let enemy_city = s.cities.iter().find(|c| {
                c.alive
                    && c.owner.is_some_and(|o| enemies.contains(&o))
                    && c.hex.distance(u.hex) <= reach
            });
            if let Some(c) = enemy_city {
                wish.push((
                    1,
                    Order::Attack {
                        army: u.id,
                        target: AttackTarget::City(c.id),
                    },
                ));
                continue;
            }
            let enemy_unit = s
                .units
                .iter()
                .filter(|e| {
                    e.alive
                        && matches!(e.owner, Owner::Civ(o) if enemies.contains(&o))
                        && e.hex.distance(u.hex) <= reach
                })
                .min_by_key(|e| e.troops);
            if let Some(e) = enemy_unit {
                wish.push((
                    1,
                    Order::Attack {
                        army: u.id,
                        target: AttackTarget::Unit(e.id),
                    },
                ));
                continue;
            }
            if Some(u.id) == garrison || enemies.is_empty() || !u.path.is_empty() {
                continue;
            }
            // March on the nearest enemy city.
            let target = s
                .cities
                .iter()
                .filter(|c| c.alive && c.owner.is_some_and(|o| enemies.contains(&o)))
                .min_by_key(|c| c.hex.distance(u.hex))
                .map(|c| c.hex);
            if let Some(goal) = target {
                if let Some(path) = path_to(s, r, civ, u.hex, |h| h.distance(goal) == 1) {
                    wish.push((3, Order::MoveUnit { unit: u.id, path }));
                }
            }
        }

        // Keep the most important orders that fit the budget, one per unit.
        wish.sort_by_key(|(p, _)| *p);
        let mut used_units = BTreeSet::new();
        let mut out = Vec::new();
        let mut cost = 0usize;
        for (_, o) in wish {
            if let Some(u) = o.commanded_unit() {
                if !used_units.insert(u) {
                    continue;
                }
            }
            let c = o.cost() as usize;
            if cost + c > budget {
                continue;
            }
            cost += c;
            out.push(o);
        }
        out
    }
}
