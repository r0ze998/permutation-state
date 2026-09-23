//! Bot match + replay recorder.
//!
//! Plays one Blitz match (180 ticks) with scripted bots on the real engine
//! and writes a JSON replay for the web viewer:
//!
//!     cargo run --release --example replay -- replay.json
//!
//! The bots are deliberately simple rule-based players with four
//! personalities. They read the full state (the engine has no fog-of-war
//! view yet), so they are a test harness, not the reference agent.

use permutation_rules::buildings::Building;
use permutation_rules::diplomacy::alliance_group;
use permutation_rules::genesis::{new_season, Entry};
use permutation_rules::hex::Hex;
use permutation_rules::invariants;
use permutation_rules::orders::{AttackTarget, Order, OrderBatch};
use permutation_rules::rng::Seed;
use permutation_rules::state::{
    CivId, DeclaredKind, Owner, ProposalKind, QueueItem, Relation, WorldState,
};
use permutation_rules::tech::{Tech, TECHS};
use permutation_rules::tick::{may_enter, resolve_tick, TickInput};
use permutation_rules::units::{stats, UnitClass, UnitType};
use permutation_rules::{Preset, Ruleset};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fmt::Write as _;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Persona {
    Warlord,
    Builder,
    Diplomat,
    Scholar,
}

impl Persona {
    fn name(self) -> &'static str {
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

const NAMES: [&str; 6] = ["Aster", "Borealis", "Cinder", "Dunmar", "Ember", "Fjordhal"];
const PERSONAS: [Persona; 6] = [
    Persona::Warlord,
    Persona::Builder,
    Persona::Diplomat,
    Persona::Scholar,
    Persona::Warlord,
    Persona::Diplomat,
];

struct Bot {
    civ: CivId,
    persona: Persona,
    war_started: HashMap<CivId, u16>,
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

fn city_count(s: &WorldState, civ: CivId) -> usize {
    s.living_cities_of(civ).count()
}

fn my_units(s: &WorldState, civ: CivId) -> impl Iterator<Item = &permutation_rules::state::Unit> {
    s.units
        .iter()
        .filter(move |u| u.alive && u.owner == Owner::Civ(civ))
}

fn troops_of(s: &WorldState, civ: CivId) -> u32 {
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
    fn orders(&mut self, s: &WorldState, r: &Ruleset) -> Vec<Order> {
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
        let _ = alliance_group; // re-exported for viewers; bots use relations directly
        out
    }
}

// ------------------------------------------------------------------ recording

fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out.push('"');
}

fn rel_code(r: Relation) -> char {
    match r {
        Relation::Peace => 'P',
        Relation::War { .. } => 'W',
        Relation::Nap { .. } => 'N',
        Relation::Alliance { .. } => 'A',
    }
}

fn frame(s: &WorldState, events: &[String]) -> String {
    let mut f = String::new();
    let n = s.civs.len() as CivId;
    write!(f, "{{\"t\":{},\"own\":\"", s.tick).unwrap();
    for t in &s.map.tiles {
        let owner = t
            .owner_city
            .and_then(|c| s.cities.get(c as usize))
            .filter(|c| c.alive)
            .and_then(|c| c.owner);
        f.push(match owner {
            Some(o) => char::from_digit(o as u32, 36).unwrap(),
            None => '.',
        });
    }
    f.push_str("\",\"cities\":[");
    let cities: Vec<String> = s
        .cities
        .iter()
        .filter(|c| c.alive)
        .map(|c| {
            format!(
                "[{},{},{},{},{},{},{},{},{}]",
                c.id,
                c.hex.q,
                c.hex.r,
                c.owner.map_or(-1, |o| o as i32),
                c.pop,
                c.defense / 1000,
                c.buildings.has(Building::Walls) as u8,
                c.buildings.star_gate_stages(),
                c.razing.is_some() as u8
            )
        })
        .collect();
    f.push_str(&cities.join(","));
    f.push_str("],\"units\":[");
    let units: Vec<String> = s
        .units
        .iter()
        .filter(|u| u.alive)
        .map(|u| {
            let owner = match u.owner {
                Owner::Civ(c) => c as i32,
                Owner::Barbarian => -2,
            };
            format!(
                "[{},{},{},{},{},{}]",
                u.id,
                u.hex.q,
                u.hex.r,
                owner,
                u.unit_type as u8,
                u.troops / 100
            )
        })
        .collect();
    f.push_str(&units.join(","));
    f.push_str("],\"civs\":[");
    let civs: Vec<String> = s
        .civs
        .iter()
        .map(|c| {
            let pop: u32 = s.living_cities_of(c.id).map(|x| x.pop).sum();
            format!(
                "[{},{},{},{},{},{},{},{},{},{},{},{}]",
                c.gold / 1000,
                c.scores.science_total,
                c.scores.dominion,
                c.scores.concord_raw,
                c.scores.star_gate_stages,
                c.techs.count(),
                c.war_weariness,
                c.is_aggressor(s.tick.saturating_sub(1), 12) as u8,
                pop,
                city_count(s, c.id),
                c.influence / 1000,
                troops_of(s, c.id)
            )
        })
        .collect();
    f.push_str(&civs.join(","));
    f.push_str("],\"rel\":\"");
    for a in 0..n {
        for b in a + 1..n {
            f.push(rel_code(s.relation(a, b)));
        }
    }
    f.push_str("\",\"cs\":[");
    let cs: Vec<String> = s
        .city_states
        .iter()
        .map(|c| {
            format!(
                "[{},{},{}]",
                c.suzerain.map_or(-1, |x| x as i32),
                c.captured_by.map_or(-1, |x| x as i32),
                c.pop
            )
        })
        .collect();
    f.push_str(&cs.join(","));
    f.push_str("],\"ev\":[");
    let mut first = true;
    for e in events {
        if !first {
            f.push(',');
        }
        first = false;
        json_str(&mut f, e);
    }
    f.push_str("]}");
    f
}

fn diff_events(prev: &WorldState, next: &WorldState, names: &[&str]) -> Vec<String> {
    let mut ev = Vec::new();
    let n = next.civs.len() as CivId;
    let nm = |c: CivId| names[c as usize];
    for a in 0..n {
        for b in a + 1..n {
            let (r0, r1) = (prev.relation(a, b), next.relation(a, b));
            if rel_code(r0) == rel_code(r1) {
                continue;
            }
            ev.push(match r1 {
                Relation::War {
                    declared_by,
                    casus_belli,
                    ..
                } => {
                    let other = if declared_by == a { b } else { a };
                    let why = if matches!(r0, Relation::Nap { .. }) {
                        " by breaking their pact"
                    } else if casus_belli {
                        " (casus belli)"
                    } else {
                        ""
                    };
                    format!(
                        "war|{} declares war on {}{}",
                        nm(declared_by),
                        nm(other),
                        why
                    )
                }
                Relation::Peace => match r0 {
                    Relation::War { .. } => format!("peace|{} and {} make peace", nm(a), nm(b)),
                    Relation::Nap { .. } => {
                        format!("diplo|The pact between {} and {} expires", nm(a), nm(b))
                    }
                    _ => format!("diplo|{} and {} end their alliance", nm(a), nm(b)),
                },
                Relation::Nap { .. } => {
                    format!("diplo|{} and {} sign a non-aggression pact", nm(a), nm(b))
                }
                Relation::Alliance { .. } => {
                    format!("ally|{} and {} form an alliance", nm(a), nm(b))
                }
            });
        }
    }
    for c in &next.cities {
        let before = prev.cities.get(c.id as usize);
        match before {
            None if c.founder != u16::MAX && c.captured_tick.is_none() => ev.push(format!(
                "found|{} founds a new city",
                nm(c.owner.unwrap_or(c.founder))
            )),
            None => ev.push(format!(
                "capture|{} conquers a city-state",
                nm(c.owner.unwrap_or(0))
            )),
            Some(b) if b.alive && !c.alive => ev.push("raze|A city is razed to a ruin".to_string()),
            Some(b) if b.alive && c.alive && b.owner != c.owner => {
                ev.push(match (b.owner, c.owner) {
                    (Some(o), Some(nw)) => {
                        format!("capture|{} captures a city of {}", nm(nw), nm(o))
                    }
                    (Some(o), None) => {
                        format!("revolt|A city of {} revolts and becomes free", nm(o))
                    }
                    (None, Some(nw)) => format!("capture|{} captures a free city", nm(nw)),
                    _ => continue,
                })
            }
            _ => {}
        }
        if let Some(b) = before {
            let (s0, s1) = (
                b.buildings.star_gate_stages(),
                c.buildings.star_gate_stages(),
            );
            if s1 > s0 {
                ev.push(format!(
                    "science|{} completes Star Gate stage {}",
                    nm(c.owner.unwrap_or(0)),
                    s1
                ));
            }
        }
    }
    for (i, cs) in next.city_states.iter().enumerate() {
        let before = &prev.city_states[i];
        if cs.suzerain != before.suzerain {
            if let Some(z) = cs.suzerain {
                ev.push(format!(
                    "diplo|{} becomes suzerain of city-state {}",
                    nm(z),
                    i + 1
                ));
            }
        }
    }
    for c in &next.civs {
        let p = &prev.civs[c.id as usize];
        for t in TECHS {
            if c.techs.has(t.tech) && !p.techs.has(t.tech) && t.era >= 3 {
                ev.push(format!("tech|{} discovers {:?}", nm(c.id), t.tech));
            }
        }
    }
    ev
}

fn main() {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "replay.json".to_string());
    let rules = Ruleset::new(Preset::Blitz);
    let world: Seed = *b"permutation-state/world/blitz-01";
    let season: Seed = *b"permutation-state/season/demo-01";
    let entries: Vec<Entry> = NAMES
        .iter()
        .enumerate()
        .map(|(i, name)| Entry {
            name: name.to_string(),
            declared_kind: if i % 2 == 0 {
                DeclaredKind::Agent
            } else {
                DeclaredKind::Human
            },
            payout_wallet: [i as u8 + 1; 32],
        })
        .collect();
    let mut s = new_season(&rules, &world, &season, &entries).expect("genesis");
    let mut bots: Vec<Bot> = PERSONAS
        .iter()
        .enumerate()
        .map(|(i, p)| Bot {
            civ: i as CivId,
            persona: *p,
            war_started: HashMap::new(),
        })
        .collect();

    let mut out = String::new();
    out.push_str("{\"meta\":{");
    write!(
        out,
        "\"preset\":\"Blitz\",\"ticks\":{},\"tickSeconds\":{},\"rulesetHash\":\"{}\",",
        rules.ticks_per_season,
        rules.tick_seconds,
        s.ruleset_hash
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
    .unwrap();
    out.push_str("\"civs\":[");
    let civs: Vec<String> = s
        .civs
        .iter()
        .map(|c| {
            let mut e = String::from("{\"name\":");
            json_str(&mut e, &c.name);
            write!(
                e,
                ",\"persona\":\"{}\",\"kind\":\"{:?}\"}}",
                PERSONAS[c.id as usize].name(),
                c.declared_kind
            )
            .unwrap();
            e
        })
        .collect();
    out.push_str(&civs.join(","));
    out.push_str("]},\"map\":{");
    write!(out, "\"radius\":{},\"tiles\":[", s.map.radius).unwrap();
    let tiles: Vec<String> = s
        .map
        .tiles
        .iter()
        .map(|t| {
            let res = match t.resource {
                None => -1,
                Some(r) => r as i32,
            };
            format!(
                "[{},{},{},{},{}]",
                t.hex.q, t.hex.r, t.terrain as u8, t.river as u8, res
            )
        })
        .collect();
    out.push_str(&tiles.join(","));
    out.push_str("]},\"cityStates\":[");
    let css: Vec<String> = s
        .city_states
        .iter()
        .map(|c| format!("[{},{},{},\"{:?}\"]", c.id, c.hex.q, c.hex.r, c.specialty))
        .collect();
    out.push_str(&css.join(","));
    out.push_str("],\"frames\":[");
    out.push_str(&frame(&s, &[]));

    let names: Vec<&str> = NAMES.to_vec();
    let mut roots = Vec::new();
    while s.tick < rules.ticks_per_season {
        let batches: Vec<OrderBatch> = bots
            .iter_mut()
            .map(|b| OrderBatch {
                civ: b.civ,
                tick: s.tick,
                decision_digest: [0; 32],
                orders: b.orders(&s, &rules),
            })
            .collect();
        let mut vrf = [0u8; 32];
        vrf[..2].copy_from_slice(&s.tick.to_le_bytes());
        let prev = s.clone();
        let root = resolve_tick(&mut s, &rules, &TickInput { vrf, batches }).expect("tick");
        let v = invariants::check(&s, &rules);
        assert!(v.is_empty(), "invariant violated at tick {}: {v:?}", s.tick);
        roots.push(root);
        let ev = diff_events(&prev, &s, &names);
        out.push(',');
        out.push_str(&frame(&s, &ev));
    }
    out.push_str("],\"finalRoot\":\"");
    out.push_str(
        &roots
            .last()
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    );
    out.push_str("\"}");
    std::fs::write(&out_path, &out).expect("write replay");
    eprintln!("wrote {} ({} bytes, {} ticks)", out_path, out.len(), s.tick);
    for c in &s.civs {
        eprintln!(
            "{:9} {:8} cities {} pop {:3} techs {:2} stages {} dom {:6} con {:6} sci {:5} troops {}",
            c.name,
            PERSONAS[c.id as usize].name(),
            city_count(&s, c.id),
            s.living_cities_of(c.id).map(|x| x.pop).sum::<u32>(),
            c.techs.count(),
            c.scores.star_gate_stages,
            c.scores.dominion,
            c.scores.concord_raw,
            c.scores.science_total,
            troops_of(&s, c.id)
        );
    }
}
