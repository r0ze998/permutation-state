//! Previews: legal actions with costs, estimates and blocked reasons.
//!
//! This is the `listActions` / `simulate` surface of V4 §5.3, shared by the
//! browser UI and agents. Every "allowed?" answer comes from `checks` or the
//! engine's own planner, so a preview can never disagree with resolution.
//! Estimates (ticks to finish, expected losses) are forecasts, not promises:
//! the tick draws variance and resolves every civ's orders together.

use crate::battle::{forecast_attack, AttackForecast};
use crate::buildings::BUILDINGS;
use crate::checks::{self, Blocked};
use crate::economy::{
    amenities, apply_amenities, city_yield, growth_threshold, tech_cost, CityYield,
};
use crate::fixed::{apply_bps, MILLI};
use crate::hex::Hex;
use crate::orders::AttackTarget;
use crate::params::Ruleset;
use crate::state::{CivId, QueueItem, WorldState};
use crate::tech::{Tech, TECHS};
use crate::units::UnitType;
use alloc::vec::Vec;

// ------------------------------------------------------------------ movement

// Reachability and paths are the engine's own (`movement`), re-exported here.
pub use crate::movement::{path_to, reachable, Reach};

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

/// Production cost in whole units: the engine's own `item_cost`.
fn item_cost(state: &WorldState, rules: &Ruleset, city: u32, item: QueueItem) -> u32 {
    (crate::tick::item_cost(state, rules, &state.cities[city as usize], &item) / MILLI) as u32
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
            K::Peace => (A::AcceptPeace, checks::accept_peace(state, civ, other)),
            // Offering the proposer's bond back, at least the minimum.
            K::Nap { bond } => {
                let mine = bond.max(rules.nap_min_bond);
                (
                    A::AcceptNap,
                    checks::accept_nap(state, rules, civ, other, mine)
                        .and_then(|()| checks::nap_bonds_payable(state, civ, other, mine, bond)),
                )
            }
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
    use crate::buildings::Building;
    use crate::genesis::{new_season, Entry};
    use crate::params::Preset;

    fn world() -> (Ruleset, WorldState) {
        let rules = Ruleset::new(Preset::Blitz);
        let entries: Vec<Entry> = (0..4)
            .map(|i| Entry {
                name: alloc::format!("c{i}"),
                treasury: 0,
            })
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
