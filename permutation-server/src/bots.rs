//! Scripted rule-based bots with four personalities (Warlord, Builder,
//! Diplomat, Scholar). They play through the same order API as a human,
//! decide from the full state like every player (perfect information), and commit a short rationale
//! each tick (§4.3). They are a sparring partner, not the reference agent.

use permutation_rules::buildings::Building;
use permutation_rules::hex::Hex;
use permutation_rules::orders::{AttackTarget, Good, Order, Side, StandingOrder, StandingTarget};
use permutation_rules::state::{
    CivId, Owner, ProposalKind, QueueItem, Relation, StandingRule, WorldState,
};
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
/// Persona of a scripted civ (seats beyond six reuse the list).
pub fn persona_of(civ: CivId) -> Persona {
    PERSONAS[civ as usize % PERSONAS.len()]
}

pub const PERSONAS: [Persona; 6] = [
    Persona::Warlord,
    Persona::Builder,
    Persona::Diplomat,
    Persona::Scholar,
    Persona::Warlord,
    Persona::Diplomat,
];

/// An AI's temperament (V5 §18.8), 0..=100 each, drawn per season on the
/// operator's server and never put on chain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Traits {
    /// Willingness to fight and to pay others to fight.
    pub aggression: u8,
    /// How much USDC a deal must bring before it is taken.
    pub greed: u8,
    /// Keeps its word (pacts, alliances) rather than selling it.
    pub loyalty: u8,
    /// Answers offers and talk at all.
    pub openness: u8,
}

impl Traits {
    /// The traits a persona has when nothing was drawn (sim, local tests).
    pub fn of(p: Persona) -> Traits {
        let (aggression, greed, loyalty, openness) = match p {
            Persona::Warlord => (80, 60, 40, 40),
            Persona::Builder => (30, 50, 60, 60),
            Persona::Diplomat => (20, 40, 70, 90),
            Persona::Scholar => (25, 45, 60, 50),
        };
        Traits {
            aggression,
            greed,
            loyalty,
            openness,
        }
    }

    /// Traits drawn from `seed` for nation `civ`.
    pub fn draw(seed: &[u8], civ: CivId) -> Traits {
        use sha2::{Digest, Sha256};
        let h: [u8; 32] = Sha256::new()
            .chain_update(b"PS/traits")
            .chain_update(seed)
            .chain_update(civ.to_le_bytes())
            .finalize()
            .into();
        Traits {
            aggression: h[0] % 101,
            greed: h[1] % 101,
            loyalty: h[2] % 101,
            openness: h[3] % 101,
        }
    }

    /// The persona that plays these traits.
    pub fn persona(self) -> Persona {
        if self.aggression >= 60 {
            Persona::Warlord
        } else if self.openness >= 60 {
            Persona::Diplomat
        } else if self.greed >= 55 {
            Persona::Builder
        } else {
            Persona::Scholar
        }
    }

    /// Least USDC (base units) a contract must pay before this AI takes it.
    pub fn price(self) -> u64 {
        500_000 + self.greed as u64 * 40_000
    }
}

/// A scripted rule-based player. Reads the full state, like every player.
pub struct Bot {
    pub civ: CivId,
    pub persona: Persona,
    pub traits: Traits,
    pub war_started: HashMap<CivId, u16>,
    /// Cities this bot marches on first when at war (sim only: the
    /// operator AIs' home cities, to measure what a leak is worth, V5 §18).
    /// Hosted bots never get any.
    pub targets: Vec<permutation_rules::state::CityId>,
}

impl Bot {
    pub fn new(civ: CivId, persona: Persona) -> Self {
        Bot {
            civ,
            persona,
            traits: Traits::of(persona),
            war_started: HashMap::new(),
            targets: Vec::new(),
        }
    }

    /// A bot whose persona and traits were drawn for the season.
    pub fn with_traits(civ: CivId, traits: Traits) -> Self {
        Bot {
            civ,
            persona: traits.persona(),
            traits,
            war_started: HashMap::new(),
            targets: Vec::new(),
        }
    }
}

// ------------------------------------------------------------------ contracts

impl Bot {
    /// Treasury contracts (V5 §18.6): answer the offers made to this nation,
    /// and offer some, by temperament. Words bind nothing; only escrowed
    /// USDC moves anyone.
    fn contracts(&self, s: &WorldState, r: &Ruleset, wish: &mut Vec<(u8, Order)>) {
        use permutation_rules::contracts::ContractTerm;
        if !r.market_enabled {
            return;
        }
        let civ = self.civ;
        let t = self.traits;
        let me = &s.civs[civ as usize];
        let n = s.civs.len() as CivId;
        // Answer offers: take them when they pay enough and the term suits.
        for c in s
            .contracts
            .iter()
            .filter(|c| c.to == Some(civ) && c.accepted.is_none() && c.offered < s.tick)
        {
            if t.openness < 20 || c.total < t.price() {
                continue;
            }
            let suits = match c.term {
                ContractTerm::Peace => {
                    t.aggression < 70 || troops_of(s, civ) <= troops_of(s, c.from)
                }
                ContractTerm::KeepNap { .. } => t.aggression < 60,
                ContractTerm::LeaveAlliance { .. } => t.loyalty < 50 && c.total >= t.price() * 2,
                ContractTerm::Capture { .. } => false,
            };
            if suits {
                wish.push((0, Order::AcceptContract { id: c.id }));
            }
        }
        // What the treasury may still put in escrow (not contract income).
        let spendable = me.usdc.saturating_sub(me.contract_income);
        let open = permutation_rules::contracts::open_offers(s, civ);
        if spendable < 1_000_000
            || open >= r.contract_max_open as usize
            || s.tick >= r.exchange_freeze_tick
        {
            return;
        }
        let last = r.ticks_per_season.saturating_sub(1);
        let offered_to = |o: CivId| s.contracts.iter().any(|c| c.from == civ && c.to == Some(o));
        // Losing a war: buy peace.
        for o in 0..n {
            if o != civ
                && s.at_war(civ, o)
                && troops_of(s, civ) < troops_of(s, o)
                && !offered_to(o)
                && t.aggression < 85
            {
                wish.push((
                    1,
                    Order::OfferContract {
                        to: Some(o),
                        term: ContractTerm::Peace,
                        usdc: (spendable / 2).min(3_000_000),
                        deadline: (s.tick + 20).min(last),
                    },
                ));
                return;
            }
        }
        // Aggressive and well funded: a bounty on the leader's capital.
        if t.aggression >= 60 && s.tick >= 40 && s.tick % 15 == 3 {
            let leader = (0..n).filter(|o| *o != civ).max_by_key(|o| {
                s.civs[*o as usize]
                    .achievements
                    .tiers
                    .iter()
                    .map(|x| *x as u32)
                    .sum::<u32>()
            });
            let capital = leader
                .and_then(|o| s.civs[o as usize].capital)
                .filter(|c| s.cities.get(*c as usize).is_some_and(|x| x.alive));
            let already = s.contracts.iter().any(|c| {
                c.from == civ
                    && matches!(c.term, ContractTerm::Capture { city } if Some(city) == capital)
            });
            if let (Some(city), false) = (capital, already) {
                wish.push((
                    2,
                    Order::OfferContract {
                        to: None,
                        term: ContractTerm::Capture { city },
                        usdc: (spendable / 3).min(2_000_000),
                        deadline: (s.tick + 40).min(last),
                    },
                ));
                return;
            }
        }
        // Loyal and open: pay a pact partner to keep it.
        if t.loyalty >= 60 && t.openness >= 50 && s.tick % 20 == 11 {
            if let Some(o) = (0..n).find(|o| {
                *o != civ && matches!(s.relation(civ, *o), Relation::Nap { .. }) && !offered_to(*o)
            }) {
                wish.push((
                    3,
                    Order::OfferContract {
                        to: Some(o),
                        term: ContractTerm::KeepNap {
                            every: 5,
                            installments: 4,
                        },
                        usdc: (spendable / 4).min(2_000_000),
                        deadline: (s.tick + 5).min(last),
                    },
                ));
            }
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
    /// Orders for this tick, from the state `s` every player sees (perfect
    /// information) and `explored`, the tiles it knows (all of them).
    pub fn orders(&mut self, s: &WorldState, r: &Ruleset, explored: &[bool]) -> Vec<Order> {
        let civ = self.civ;
        let me = &s.civs[civ as usize];
        let n = s.civs.len() as CivId;
        // Each office has its own budget and bank (V5 §5.2).
        let budget: [usize; 4] = core::array::from_fn(|i| {
            permutation_rules::orders::spendable(s, r, civ, permutation_rules::gov::Role::ALL[i])
                as usize
        });
        // (priority, order): lower number = more important.
        let mut wish: Vec<(u8, Order)> = Vec::new();

        // Treaty partners (NAP or alliance) and capital distances, for the
        // warlord's one pact with a civ it does not mean to attack.
        let partners = (0..n)
            .filter(|o| {
                *o != civ
                    && matches!(
                        s.relation(civ, *o),
                        Relation::Nap { .. } | Relation::Alliance { .. }
                    )
            })
            .count();
        let cap_distance = |o: CivId| -> Option<u32> {
            let mine = s.cities.get(me.capital? as usize)?.hex;
            let theirs = s
                .cities
                .get(s.civs[o as usize].capital? as usize)
                .filter(|c| c.alive)?;
            Some(theirs.hex.distance(mine))
        };
        // The warlord's next target is the nearest civ at peace; any other
        // civ at peace is a pact candidate.
        let not_target = |o: CivId| {
            let Some(d) = cap_distance(o) else {
                return false;
            };
            (0..n).any(|x| {
                x != civ
                    && x != o
                    && matches!(s.relation(civ, x), Relation::Peace)
                    && cap_distance(x).is_some_and(|dx| dx < d)
            })
        };

        // ---- diplomacy: answer proposals addressed to me
        for p in s
            .proposals
            .iter()
            .filter(|p| p.to == civ && p.tick < s.tick)
        {
            let accept = match (p.kind, self.persona) {
                (ProposalKind::Peace, Persona::Warlord) => troops_of(s, civ) < troops_of(s, p.from),
                (ProposalKind::Peace, _) => true,
                (ProposalKind::Nap { .. }, Persona::Warlord) => {
                    partners == 0 && me.gold >= 40_000 && not_target(p.from)
                }
                (ProposalKind::Nap { .. }, _) => me.gold >= 40_000,
                (ProposalKind::Alliance, Persona::Diplomat | Persona::Builder) => true,
                (ProposalKind::Alliance, _) => false,
            };
            if accept {
                let o = match p.kind {
                    ProposalKind::Peace => Order::AcceptPeace { civ: p.from },
                    ProposalKind::Nap { .. } => Order::AcceptNap {
                        civ: p.from,
                        bond: r.nap_min_bond,
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
                    // Living capitals only.
                    let oc = s.civs[o as usize].capital?;
                    let c = s.cities.get(oc as usize).filter(|c| c.alive)?;
                    Some((c.hex.distance(cap), o))
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
                    // Holding a city taken from them: make peace and keep it.
                    let holds_theirs = s
                        .cities
                        .iter()
                        .any(|c| c.alive && c.owner == Some(civ) && c.captured_from == Some(o));
                    let long = s.tick - since >= if holds_theirs { 20 } else { 45 };
                    if s.at_war(civ, o) && (long || troops_of(s, civ) < 4) {
                        wish.push((1, Order::ProposePeace { civ: o }));
                    }
                }
                // One pact with the farthest civ at peace, to keep a treaty
                // partner while fighting the nearest.
                let offered = |o: CivId| s.proposals.iter().any(|p| p.from == civ && p.to == o);
                if s.tick % 5 == 0 && partners == 0 && me.gold >= 50_000 {
                    if let Some(t) = (0..n)
                        .filter(|o| {
                            *o != civ
                                && matches!(s.relation(civ, *o), Relation::Peace)
                                && !offered(*o)
                                && not_target(*o)
                        })
                        .filter_map(|o| Some((cap_distance(o)?, o)))
                        .max()
                        .map(|(_, o)| o)
                    {
                        wish.push((
                            2,
                            Order::ProposeNap {
                                civ: t,
                                bond: r.nap_min_bond,
                            },
                        ));
                    }
                }
            }
            Persona::Diplomat | Persona::Builder | Persona::Scholar => {
                // Keep up to three treaty partners: offer non-aggression pacts to
                // the nearest civs at peace, and (diplomats) turn pacts into alliances.
                let offered = |o: CivId| s.proposals.iter().any(|p| p.from == civ && p.to == o);
                if s.tick % 5 == 0 && partners < 3 && me.gold >= 50_000 {
                    if let Some(t) = nearest_civ(&|o| {
                        matches!(s.relation(civ, o), Relation::Peace) && !offered(o)
                    }) {
                        wish.push((
                            2,
                            Order::ProposeNap {
                                civ: t,
                                bond: r.nap_min_bond,
                            },
                        ));
                    }
                }
                if self.persona == Persona::Diplomat && s.tick % 10 == 5 {
                    if let Some(t) = nearest_civ(&|o| {
                        matches!(s.relation(civ, o), Relation::Nap { .. }) && !offered(o)
                    }) {
                        wish.push((1, Order::ProposeAlliance { civ: t }));
                    }
                }
            }
        }
        for o in 0..n {
            if s.at_war(civ, o) && self.persona != Persona::Warlord {
                wish.push((1, Order::ProposePeace { civ: o }));
            }
        }
        self.contracts(s, r, &mut wish);

        // ---- envoys: every persona courts a city-state, diplomats earliest.
        // The target is the free city-state where this civ is closest to
        // (or already is) suzerain, then the nearest.
        let envoy_at = match self.persona {
            Persona::Diplomat => 15_000,
            Persona::Warlord => 30_000,
            _ => 30_000,
        };
        if me.influence >= envoy_at {
            if let Some(cap) = my_cap {
                if let Some(cs) = s
                    .city_states
                    .iter()
                    .filter(|cs| cs.captured_by.is_none() && cs.suzerain.is_none_or(|z| z == civ))
                    .min_by_key(|cs| {
                        (
                            std::cmp::Reverse(cs.influence[civ as usize]),
                            cs.hex.distance(cap),
                        )
                    })
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

        // ---- trade (concord): the gold market for surplus or missing iron
        // and horses, and small gold gifts to treaty partners and allies.
        if me.techs.has(permutation_rules::tech::Tech::Currency) && s.tick % 5 == 2 {
            for (good, stock) in [(Good::Iron, me.iron), (Good::Horses, me.horses)] {
                let whole = (stock / 1000) as u32;
                if whole > 20 {
                    wish.push((
                        4,
                        Order::MarketTrade {
                            good,
                            side: Side::Sell,
                            amount: whole - 10,
                            limit_gold: 0,
                        },
                    ));
                } else if whole < 5 && me.gold > 150_000 {
                    wish.push((
                        4,
                        Order::MarketTrade {
                            good,
                            side: Side::Buy,
                            amount: 5,
                            limit_gold: (me.gold / 1000 / 3) as u32,
                        },
                    ));
                }
            }
        }
        if s.tick % 10 == 7 && me.gold > 100_000 {
            let friends: Vec<CivId> = (0..n)
                .filter(|o| {
                    *o != civ
                        && matches!(
                            s.relation(civ, *o),
                            Relation::Nap { .. } | Relation::Alliance { .. }
                        )
                })
                .collect();
            let gift = (me.last.gold / 2).max(1);
            for o in friends.into_iter().take(2) {
                wish.push((
                    4,
                    Order::Transfer {
                        civ: o,
                        good: Good::Gold,
                        amount: gift,
                    },
                ));
            }
        }

        // ---- research: keep three techs queued
        if me.research_queue.len() < 3 {
            let mut plan = me.techs;
            for t in &me.research_queue {
                plan.insert(*t);
            }
            let mut q = me.research_queue.clone();
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
            if q.len() > me.research_queue.len() {
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
                    // Ties by a per-season random key, not the tile index:
                    // index order depends on direction, which would favour
                    // some positions of a symmetric map over others.
                    (
                        site_value(s, t.hex),
                        std::cmp::Reverse(t.hex.distance(u.hex)),
                        permutation_rules::rng::rand_id(
                            &s.season_seed,
                            b"bot-site",
                            ((t.hex.q as i64 as u64) << 32)
                                ^ (t.hex.r as i64 as u64 & 0xffff_ffff)
                                ^ ((civ as u64) << 48),
                        ),
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

        // ---- scouts: walk to the nearest tile never seen
        for u in my_units(s, civ).filter(|u| u.unit_type == UnitType::Scout && u.path.is_empty()) {
            let unexplored = |h: Hex| s.map.index_of(h).is_some_and(|i| !explored[i]);
            if let Some(path) = path_to(s, r, civ, u.hex, unexplored) {
                wish.push((3, Order::MoveUnit { unit: u.id, path }));
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
                .min_by_key(|c| (!self.targets.contains(&c.id), c.hex.distance(u.hex)))
                .map(|c| c.hex);
            if let Some(goal) = target {
                if let Some(path) = path_to(s, r, civ, u.hex, |h| h.distance(goal) == 1) {
                    wish.push((3, Order::MoveUnit { unit: u.id, path }));
                }
            }
        }

        // ---- standing rules (§13): set once, then they run for free every tick
        for u in my_units(s, civ)
            .filter(|u| !u.unit_type.is_civilian() && u.standing == StandingRule::None)
        {
            let rule = if Some(u.id) == garrison {
                StandingOrder::AutoDefend { radius: 2 }
            } else if self.persona != Persona::Warlord {
                StandingOrder::Retreat { ratio_bps: 15_000 }
            } else {
                continue;
            };
            wish.push((
                4,
                Order::SetStanding {
                    target: StandingTarget::Unit(u.id),
                    rule,
                },
            ));
        }
        if matches!(self.persona, Persona::Builder | Persona::Scholar) && me.gold > 120_000 {
            if let Some(cap) = me
                .capital
                .filter(|c| s.cities[*c as usize].standing.auto_purchase == 0)
            {
                wish.push((
                    4,
                    Order::SetStanding {
                        target: StandingTarget::City(cap),
                        rule: StandingOrder::AutoPurchase { max_gold: 30 },
                    },
                ));
            }
        }

        // Keep the most important orders that fit each office's budget, one per unit.
        wish.sort_by_key(|(p, _)| *p);
        let mut used_units = BTreeSet::new();
        let mut out = Vec::new();
        let mut cost = [0usize; 4];
        for (_, o) in wish {
            if let Some(u) = o.commanded_unit() {
                if !used_units.insert(u) {
                    continue;
                }
            }
            let Some(role) = permutation_rules::gov::Role::ALL
                .into_iter()
                .find(|x| permutation_rules::orders::role_allows(s, *x, &o))
            else {
                continue;
            };
            let c = o.cost() as usize;
            if cost[role.index()] + c > budget[role.index()] {
                continue;
            }
            cost[role.index()] += c;
            out.push(o);
        }
        out
    }
}

// ------------------------------------------------------------------ rationale

const NAMES_JA: [&str; 6] = [
    "アステル",
    "ボレアリス",
    "シンダー",
    "ダンマール",
    "エンバー",
    "フィヨルダル",
];

fn civ_ja(c: CivId) -> &'static str {
    NAMES_JA.get(c as usize).copied().unwrap_or("?")
}

// Exhaustive matches, so a new tech, building or unit does not compile
// until it has a name here.
fn tech_ja(t: Tech) -> &'static str {
    use Tech::*;
    match t {
        Agriculture => "農業",
        BronzeWorking => "青銅器",
        Archery => "弓術",
        HorsebackRiding => "騎乗",
        Masonry => "石工",
        Mysticism => "神秘主義",
        Writing => "筆記",
        Currency => "通貨",
        IronWorking => "製鉄",
        Mathematics => "数学",
        Chivalry => "騎士道",
        Philosophy => "哲学",
        Engineering => "工学",
        Astronomy => "天文学",
        Physics => "物理学",
        CelestialMechanics => "天体力学",
    }
}

fn building_ja(b: Building) -> &'static str {
    use Building::*;
    match b {
        Granary => "穀物庫",
        Workshop => "工房",
        Temple => "神殿",
        Market => "市場",
        Academy => "学術院",
        Barracks => "兵舎",
        Walls => "城壁",
        StarGate1 => "スターゲートI",
        StarGate2 => "スターゲートII",
        StarGate3 => "スターゲートIII",
    }
}

fn unit_ja(u: UnitType) -> &'static str {
    use UnitType::*;
    match u {
        Spearman => "槍兵",
        Archer => "弓兵",
        Horseman => "騎兵",
        Pikeman => "長槍兵",
        Crossbowman => "弩兵",
        Knight => "騎士",
        Scout => "斥候",
        Settler => "開拓者",
    }
}

fn item_ja(i: &QueueItem) -> String {
    match i {
        QueueItem::Building(b) => building_ja(*b).to_string(),
        QueueItem::Troops { unit, n } => format!("{}×{}", unit_ja(*unit), n),
        QueueItem::Scout => "斥候".to_string(),
        QueueItem::Settler => "開拓者".to_string(),
    }
}

impl Bot {
    /// Self-declared policy id for the decision digest.
    pub fn policy(&self) -> String {
        format!("bot/{}@1", self.persona.name().to_lowercase())
    }

    /// A short, honest summary of why these orders were chosen, written from
    /// the same belief state the orders came from. Committed before the tick
    /// resolves and revealed after it.
    pub fn rationale(&self, s: &WorldState, orders: &[Order]) -> String {
        let civ = self.civ;
        let mine = troops_of(s, civ);
        let mut parts: Vec<String> = Vec::new();
        let mut moves = 0;
        let mut scouting = 0;
        for o in orders {
            match o {
                Order::DeclareWar { civ: t } => parts.push(format!(
                    "{}に宣戦：見えている首都で最も近く、自軍{}に対し目視の相手兵{}",
                    civ_ja(*t),
                    mine,
                    troops_of(s, *t)
                )),
                Order::ProposePeace { civ: t } => {
                    parts.push(format!("{}に講和を提案：戦争を長引かせない", civ_ja(*t)))
                }
                Order::AcceptPeace { civ: t } => parts.push(format!("{}の講和を受諾", civ_ja(*t))),
                Order::AcceptNap { civ: t, .. } => {
                    parts.push(format!("{}の不可侵条約を受諾：金に余裕あり", civ_ja(*t)))
                }
                Order::ProposeNap { civ: t, .. } => parts.push(format!(
                    "{}に不可侵条約を提案：国境を落ち着かせる",
                    civ_ja(*t)
                )),
                Order::ProposeAlliance { civ: t } => {
                    parts.push(format!("{}に同盟を提案：同じ外交方針", civ_ja(*t)))
                }
                Order::AcceptAlliance { civ: t } => {
                    parts.push(format!("{}の同盟に参加", civ_ja(*t)))
                }
                Order::Attack { target, .. } => parts.push(match target {
                    AttackTarget::City(id) => format!("都市{}を攻撃：射程内の敵都市を優先", id),
                    AttackTarget::Unit(_) => "射程内で最も弱い敵部隊を攻撃".to_string(),
                    AttackTarget::CityState(id) => format!("都市国家{}を攻撃", id + 1),
                }),
                Order::FoundCity { .. } => {
                    parts.push("開拓者が良い立地に着いたので都市を建設".to_string())
                }
                Order::SetResearch { techs } => parts.push(format!(
                    "研究：{}",
                    techs
                        .iter()
                        .map(|t| tech_ja(*t))
                        .collect::<Vec<_>>()
                        .join("→")
                )),
                Order::SetQueue { city, items } => parts.push(format!(
                    "都市{}の生産：{}",
                    city,
                    if items.is_empty() {
                        "なし".to_string()
                    } else {
                        items.iter().map(item_ja).collect::<Vec<_>>().join("→")
                    }
                )),
                Order::SendEnvoy { city_state, .. } => {
                    parts.push(format!("最寄りの都市国家{}へ使節", city_state + 1))
                }
                Order::Purchase { .. } => parts.push("余った金で首都の生産を購入".to_string()),
                Order::SetStanding { rule, .. } => parts.push(match rule {
                    StandingOrder::AutoDefend { radius } => {
                        format!("首都の守備隊に自動防衛（半径{radius}）")
                    }
                    StandingOrder::Retreat { .. } => {
                        "野戦軍に撤退ルール：1.5倍の敵で後退".to_string()
                    }
                    StandingOrder::AutoPurchase { max_gold } => {
                        format!("首都で毎ティック{max_gold}金まで自動購入")
                    }
                    _ => "継続命令を更新".to_string(),
                }),
                Order::MoveUnit { unit, .. } => {
                    if s.units
                        .get(*unit as usize)
                        .is_some_and(|u| u.unit_type == UnitType::Scout)
                    {
                        scouting += 1;
                    } else {
                        moves += 1;
                    }
                }
                _ => {}
            }
        }
        if scouting > 0 {
            parts.push("斥候で未踏の地を探索".to_string());
        }
        if moves > 0 {
            parts.push(format!("部隊{}つを移動（開拓地・進軍）", moves));
        }
        if parts.is_empty() {
            parts.push("様子見：急ぐ判断なし".to_string());
        }
        format!("[{}] {}", self.persona.name(), parts.join(" / "))
    }
}
