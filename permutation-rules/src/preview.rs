//! Previews: legal actions with costs, estimates and blocked reasons.
//!
//! This is the `listActions` / `simulate` surface of V4 §5.3, shared by the
//! browser UI and agents. Every "allowed?" answer comes from `checks` or the
//! engine's own planner, so a preview can never disagree with resolution.
//! Estimates (ticks to finish, expected losses) are forecasts, not promises:
//! the tick draws variance and resolves every civ's orders together.

use crate::battle::{forecast_attack, AttackForecast};
use crate::buildings::{Building, BUILDINGS};
use crate::checks::{self, Blocked};
use crate::economy::{
    amenities, apply_amenities, city_yield, growth_threshold, stalemate_multiplier, tech_cost,
    CityYield,
};
use crate::fixed::{apply_bps, MILLI};
use crate::hex::Hex;
use crate::orders::AttackTarget;
use crate::params::Ruleset;
use crate::state::{CivId, Owner, QueueItem, WorldState};
use crate::tech::{Tech, TECHS};
use crate::tick::may_enter;
use crate::units::{stats, UnitType};
use alloc::collections::{BTreeMap, BinaryHeap};
use alloc::vec::Vec;
use core::cmp::Reverse;

// ------------------------------------------------------------------ movement

/// A tile a unit can reach, with the movement cost and ticks needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reach {
    pub hex: Hex,
    pub cost: u32,
    pub ticks: u32,
    pub steps: u32,
}

fn step_cost(state: &WorldState, unit: UnitType, h: Hex) -> Option<u32> {
    let t = state.map.tile(h)?;
    if !t.terrain.is_passable() {
        return None;
    }
    Some(if unit == UnitType::Scout {
        1
    } else {
        t.terrain.info().move_cost as u32
    })
}

/// Dijkstra over enterable tiles (§7.3), up to `max_path_len` steps. Tiles
/// holding a foreign unit are not entered (that needs `Attack`). Returns the
/// predecessor map and the reach table.
fn search(
    state: &WorldState,
    rules: &Ruleset,
    unit: u32,
) -> (BTreeMap<Hex, Hex>, BTreeMap<Hex, Reach>) {
    let mut prev = BTreeMap::new();
    let mut best: BTreeMap<Hex, Reach> = BTreeMap::new();
    let Some(u) = state.units.get(unit as usize).filter(|u| u.alive) else {
        return (prev, best);
    };
    let mover = match u.owner {
        Owner::Civ(c) => Some(c),
        Owner::Barbarian => None,
    };
    let mp = stats(u.unit_type).movement.max(1) as u32;
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u32, 0u32, u.hex)));
    best.insert(
        u.hex,
        Reach {
            hex: u.hex,
            cost: 0,
            ticks: 0,
            steps: 0,
        },
    );
    while let Some(Reverse((cost, steps, h))) = heap.pop() {
        if best.get(&h).is_some_and(|r| r.cost < cost) || steps >= rules.max_path_len as u32 {
            continue;
        }
        for n in h.neighbors() {
            let Some(c) = step_cost(state, u.unit_type, n) else {
                continue;
            };
            if !may_enter(state, rules, mover, n) {
                continue;
            }
            if state
                .units
                .iter()
                .any(|o| o.alive && o.hex == n && o.owner != u.owner)
            {
                continue;
            }
            let nc = cost + c;
            if best.get(&n).is_none_or(|r| nc < r.cost) {
                best.insert(
                    n,
                    Reach {
                        hex: n,
                        cost: nc,
                        ticks: nc.div_ceil(mp),
                        steps: steps + 1,
                    },
                );
                prev.insert(n, h);
                heap.push(Reverse((nc, steps + 1, n)));
            }
        }
    }
    best.remove(&u.hex);
    (prev, best)
}

/// Every tile the unit can reach, cheapest first.
pub fn reachable(state: &WorldState, rules: &Ruleset, unit: u32) -> Vec<Reach> {
    let mut v: Vec<Reach> = search(state, rules, unit).1.into_values().collect();
    v.sort_by_key(|r| (r.cost, r.hex));
    v
}

/// Cheapest legal path to `goal` (excluding the start), as a `MoveUnit` path.
pub fn path_to(state: &WorldState, rules: &Ruleset, unit: u32, goal: Hex) -> Option<Vec<Hex>> {
    let (prev, best) = search(state, rules, unit);
    best.get(&goal)?;
    let start = state.units.get(unit as usize)?.hex;
    let mut path = alloc::vec![goal];
    let mut cur = goal;
    while let Some(p) = prev.get(&cur) {
        if *p == start {
            break;
        }
        path.push(*p);
        cur = *p;
    }
    path.reverse();
    Some(path)
}

// ------------------------------------------------------------------ attacks

/// One thing a unit could attack this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttackOption {
    pub target: AttackTarget,
    pub hex: Hex,
    pub result: Result<AttackForecast, Blocked>,
}

/// Units, cities and city-states within 2 tiles (the longest range), each
/// with the engine's own verdict and an expected-loss forecast.
pub fn attack_options(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    army: u32,
) -> Vec<AttackOption> {
    let Some(a) = state.units.get(army as usize).filter(|u| u.alive) else {
        return Vec::new();
    };
    let near = |h: Hex| h.distance(a.hex) <= 2 && h != a.hex;
    let mut out = Vec::new();
    for t in state
        .units
        .iter()
        .filter(|t| t.alive && near(t.hex) && t.owner != a.owner)
    {
        let target = AttackTarget::Unit(t.id);
        out.push(AttackOption {
            target,
            hex: t.hex,
            result: forecast_attack(state, rules, civ, army, target),
        });
    }
    for c in state
        .cities
        .iter()
        .filter(|c| c.alive && near(c.hex) && c.owner != Some(civ))
    {
        let target = AttackTarget::City(c.id);
        out.push(AttackOption {
            target,
            hex: c.hex,
            result: forecast_attack(state, rules, civ, army, target),
        });
    }
    for cs in state
        .city_states
        .iter()
        .filter(|c| c.captured_by.is_none() && near(c.hex))
    {
        let target = AttackTarget::CityState(cs.id);
        out.push(AttackOption {
            target,
            hex: cs.hex,
            result: forecast_attack(state, rules, civ, army, target),
        });
    }
    out
}

// ------------------------------------------------------------------ cities

/// A city's per-tick outlook, from the engine's own yield functions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CityOutlook {
    pub yields: CityYield,
    pub amenities: i32,
    /// Net food per tick after consumption and amenity effects.
    pub food_surplus: i64,
    /// Production per tick after amenity effects (whole units).
    pub production: i64,
    /// Ticks until the next population point, if growing.
    pub ticks_to_grow: Option<u32>,
    pub growth_threshold: u64,
}

pub fn city_outlook(state: &WorldState, rules: &Ruleset, city: u32) -> Option<CityOutlook> {
    let c = state.cities.get(city as usize).filter(|c| c.alive)?;
    let civ = &state.civs[c.owner? as usize];
    let (y, _) = city_yield(
        &state.map,
        c,
        civ.capital == Some(c.id),
        civ.techs.has(Tech::Philosophy),
    );
    let am = amenities(c, state.city_count(civ.id), civ.war_weariness);
    let (surplus, prod_bps) = apply_amenities(rules, y.food, c.pop, am);
    let threshold = growth_threshold(c.pop);
    let need = threshold as i64 * MILLI - c.food;
    let ticks_to_grow = (surplus > 0).then(|| (need.max(0) / (surplus * MILLI)) as u32 + 1);
    Some(CityOutlook {
        yields: y,
        amenities: am,
        food_surplus: surplus,
        production: apply_bps(y.prod as i64, prod_bps),
        ticks_to_grow,
        growth_threshold: threshold,
    })
}

/// A production item a city could queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueOption {
    pub item: QueueItem,
    /// Production cost (whole units) at this tick, incl. Barracks and the stalemate breaker.
    pub cost: u32,
    /// Ticks to finish from the city's current store at its current production.
    pub ticks: Option<u32>,
    pub result: Result<(), Blocked>,
}

fn item_cost(state: &WorldState, rules: &Ruleset, city: u32, item: QueueItem) -> u32 {
    let c = &state.cities[city as usize];
    match item {
        QueueItem::Building(b) => {
            let base = crate::buildings::info(b).prod_cost;
            if b.is_star_gate() {
                apply_bps(base as i64, stalemate_multiplier(rules, state.tick)) as u32
            } else {
                base
            }
        }
        QueueItem::Troops { unit, n } => {
            let base = stats(unit).prod_cost * n as u32;
            if c.buildings.has(Building::Barracks) {
                base * 3 / 4
            } else {
                base
            }
        }
        QueueItem::Scout => stats(UnitType::Scout).prod_cost,
        QueueItem::Settler => stats(UnitType::Settler).prod_cost,
    }
}

/// Every building, a 5-troop army of each type, Scout and Settler.
pub fn queue_options(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    city: u32,
) -> Vec<QueueOption> {
    let Some(c) = state.cities.get(city as usize).filter(|c| c.alive) else {
        return Vec::new();
    };
    let prod = city_outlook(state, rules, city).map_or(0, |o| o.production.max(0));
    let store = c.prod / MILLI;
    let mut items: Vec<QueueItem> = BUILDINGS
        .iter()
        .map(|b| QueueItem::Building(b.building))
        .collect();
    for u in [
        UnitType::Spearman,
        UnitType::Archer,
        UnitType::Horseman,
        UnitType::Pikeman,
        UnitType::Crossbowman,
        UnitType::Knight,
    ] {
        items.push(QueueItem::Troops { unit: u, n: 5 });
    }
    items.push(QueueItem::Scout);
    items.push(QueueItem::Settler);
    items
        .into_iter()
        .map(|item| {
            let cost = item_cost(state, rules, city, item);
            let mut result = checks::queue_item(state, civ, city, &item);
            if result.is_ok() && item == QueueItem::Settler && c.pop < rules.settler_min_pop {
                result = Err(Blocked::NeedsPop {
                    need: rules.settler_min_pop,
                    have: c.pop,
                });
            }
            let left = (cost as i64 - store).max(0);
            let ticks = if left == 0 {
                Some(1)
            } else if prod > 0 {
                Some(((left + prod - 1) / prod) as u32)
            } else {
                None
            };
            QueueOption {
                item,
                cost,
                ticks,
                result,
            }
        })
        .collect()
}

// ------------------------------------------------------------------ research

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResearchOption {
    pub tech: Tech,
    pub cost: u64,
    pub result: Result<(), Blocked>,
}

pub fn research_options(state: &WorldState, rules: &Ruleset, civ: CivId) -> Vec<ResearchOption> {
    let c = &state.civs[civ as usize];
    let cities = state.city_count(civ);
    TECHS
        .iter()
        .map(|t| ResearchOption {
            tech: t.tech,
            cost: tech_cost(rules, t.tech, cities),
            result: checks::research(c.techs, t.tech),
        })
        .collect()
}

// ------------------------------------------------------------------ diplomacy

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiplomaticAction {
    DeclareWar,
    ProposePeace,
    ProposeNap,
    ProposeAlliance,
    AcceptPeace,
    AcceptNap,
    AcceptAlliance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiplomacyOption {
    pub action: DiplomaticAction,
    pub result: Result<(), Blocked>,
}

/// What `civ` can do towards `other` this tick. Accepts are listed only when a
/// matching earlier-tick proposal exists.
pub fn diplomacy_options(
    state: &WorldState,
    rules: &Ruleset,
    civ: CivId,
    other: CivId,
) -> Vec<DiplomacyOption> {
    use crate::state::ProposalKind as K;
    use DiplomaticAction as A;
    let mut v = alloc::vec![
        DiplomacyOption {
            action: A::DeclareWar,
            result: checks::declare_war(state, civ, other)
        },
        DiplomacyOption {
            action: A::ProposePeace,
            result: checks::propose_peace(state, civ, other)
        },
        DiplomacyOption {
            action: A::ProposeNap,
            result: checks::propose_nap(state, rules, civ, other, rules.nap_min_bond),
        },
        DiplomacyOption {
            action: A::ProposeAlliance,
            result: checks::propose_alliance(state, rules, civ, other)
        },
    ];
    for p in state
        .proposals
        .iter()
        .filter(|p| p.from == other && p.to == civ && p.tick < state.tick)
    {
        let (action, result) = match p.kind {
            K::Peace => (A::AcceptPeace, checks::propose_peace(state, other, civ)),
            K::Nap { bond } => (
                A::AcceptNap,
                checks::propose_nap(state, rules, civ, other, bond.max(rules.nap_min_bond)),
            ),
            K::Alliance => (
                A::AcceptAlliance,
                checks::join_alliance(state, rules, civ, other),
            ),
        };
        v.push(DiplomacyOption { action, result });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::{new_season, Entry};
    use crate::params::Preset;

    fn world() -> (Ruleset, WorldState) {
        let rules = Ruleset::new(Preset::Blitz);
        let entries: Vec<Entry> = (0..4)
            .map(|i| Entry { name: alloc::format!("c{i}"), treasury: 0 })
            .collect();
        let s = new_season(&rules, &[11; 32], &[22; 32], &entries).unwrap();
        (rules, s)
    }

    #[test]
    fn paths_lead_to_their_goal_through_reachable_tiles() {
        let (rules, s) = world();
        let scout = s
            .units
            .iter()
            .find(|u| u.unit_type == UnitType::Scout)
            .unwrap();
        let reach = reachable(&s, &rules, scout.id);
        assert!(!reach.is_empty());
        let far = reach.iter().max_by_key(|r| r.cost).unwrap();
        let path = path_to(&s, &rules, scout.id, far.hex).unwrap();
        assert_eq!(*path.last().unwrap(), far.hex);
        assert_eq!(path.len() as u32, far.steps);
        let mut prev = scout.hex;
        for h in &path {
            assert_eq!(prev.distance(*h), 1);
            prev = *h;
        }
    }

    #[test]
    fn queue_options_explain_blocked_items() {
        let (rules, s) = world();
        let city = s.civs[0].capital.unwrap();
        let opts = queue_options(&s, &rules, 0, city);
        let find = |item| opts.iter().find(|o| o.item == item).unwrap();
        assert_eq!(find(QueueItem::Building(Building::Granary)).result, Ok(()));
        assert_eq!(
            find(QueueItem::Building(Building::Temple)).result,
            Err(Blocked::NeedsTech(Tech::Mysticism))
        );
        assert_eq!(
            find(QueueItem::Settler).result,
            Err(Blocked::NeedsPop { need: 2, have: 1 })
        );
        assert!(find(QueueItem::Building(Building::Granary)).ticks.is_some());
    }

    #[test]
    fn research_and_diplomacy_options_reuse_the_engine_checks() {
        let (rules, s) = world();
        let r = research_options(&s, &rules, 0);
        assert_eq!(
            r.iter()
                .find(|o| o.tech == Tech::Agriculture)
                .unwrap()
                .result,
            Ok(())
        );
        assert_eq!(
            r.iter().find(|o| o.tech == Tech::Writing).unwrap().result,
            Err(Blocked::NeedsTech(Tech::Agriculture))
        );
        let d = diplomacy_options(&s, &rules, 0, 1);
        assert_eq!(d[0].result, Ok(())); // DeclareWar from Peace
        assert_eq!(d[1].result, Err(Blocked::NotAtWar)); // ProposePeace
    }
}
