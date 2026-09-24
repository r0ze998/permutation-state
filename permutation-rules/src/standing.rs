//! Standing rules (§13): first-party automation, identical for humans and agents.
//!
//! `SetStanding` costs one order and is applied in phase 2. In phase 3 every
//! rule is compiled into *implicit orders* (`WorldState::implicit`) that
//! phases 4 (movement) and 5 (combat) treat exactly like manual ones.
//! Execution is free. A unit that received a manual order this tick skips
//! its rule for that tick.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use crate::hex::Hex;
use crate::orders::{AttackTarget, Order, StandingOrder, StandingTarget};
use crate::params::Ruleset;
use crate::state::{CityStanding, CivId, Owner, StandingRule, WorldState};
use crate::tick::{accepted, TickInput};
use crate::units::stats;

/// Phase 2: apply a validated `SetStanding`.
pub(crate) fn apply(state: &mut WorldState, civ: CivId, target: StandingTarget, rule: StandingOrder) {
    if crate::checks::standing(state, civ, target, &rule).is_err() {
        return; // invalid at resolution: dropped without refund (§4.1)
    }
    match target {
        StandingTarget::Unit(id) => {
            let u = &mut state.units[id as usize];
            u.standing = match rule {
                StandingOrder::AutoDefend { radius } => StandingRule::AutoDefend { radius, anchor: u.hex },
                StandingOrder::Retreat { ratio_bps } => StandingRule::Retreat { ratio_bps },
                StandingOrder::Patrol { route } => {
                    let mut r = [u.hex; 6];
                    r[..route.len()].copy_from_slice(&route);
                    StandingRule::Patrol { route: r, len: route.len() as u8, next: 0 }
                }
                _ => StandingRule::None,
            };
        }
        StandingTarget::City(id) => {
            let c = &mut state.cities[id as usize];
            match rule {
                StandingOrder::QueueRepeat { on } => c.standing.repeat_queue = on,
                StandingOrder::AutoPurchase { max_gold } => c.standing.auto_purchase = max_gold,
                _ => c.standing = CityStanding::DEFAULT,
            }
        }
    }
}

/// Combat strength of a living army: troops × unit strength.
fn strength(state: &WorldState, id: usize) -> u64 {
    let u = &state.units[id];
    u.troops as u64 * stats(u.unit_type).strength as u64
}

/// Phase 3: compile every rule into implicit orders (and apply AutoPurchase).
pub(crate) fn phase_standing(state: &mut WorldState, rules: &Ruleset, input: &TickInput) {
    state.implicit.clear();
    let mut manual: BTreeSet<u32> = BTreeSet::new();
    let mut manual_purchase: BTreeSet<u32> = BTreeSet::new();
    for (_, _, orders) in accepted(state, rules, input) {
        for o in &orders {
            if let Some(u) = o.commanded_unit() {
                manual.insert(u);
            }
            if let Order::Attack { army, .. } = o {
                manual.insert(*army);
            }
            if let Order::Purchase { city, .. } = o {
                manual_purchase.insert(*city);
            }
        }
    }

    let mut implicit: Vec<(CivId, Order)> = Vec::new();
    for i in 0..state.units.len() {
        let u = &state.units[i];
        let Owner::Civ(civ) = u.owner else { continue };
        if !u.alive || manual.contains(&u.id) || u.standing == StandingRule::None {
            continue;
        }
        match u.standing {
            StandingRule::None => {}
            StandingRule::AutoDefend { radius, anchor } => {
                let reach = stats(u.unit_type).range.max(1) as u32;
                let target = state
                    .units
                    .iter()
                    .filter(|e| {
                        e.alive
                            && !e.unit_type.is_civilian()
                            && crate::battle::hostile(state, civ, e.owner)
                            && e.hex.distance(u.hex) <= reach
                            && e.hex.distance(anchor) <= radius as u32
                    })
                    .min_by_key(|e| (e.troops, e.id));
                if let Some(e) = target {
                    implicit.push((civ, Order::Attack { army: u.id, target: AttackTarget::Unit(e.id) }));
                }
            }
            StandingRule::Retreat { ratio_bps } => {
                let own = strength(state, i);
                let enemy: u64 = (0..state.units.len())
                    .filter(|&j| {
                        let e = &state.units[j];
                        e.alive
                            && !e.unit_type.is_civilian()
                            && crate::battle::hostile(state, civ, e.owner)
                            && e.hex.distance(u.hex) == 1
                    })
                    .map(|j| strength(state, j))
                    .sum();
                if own == 0 || enemy * 10_000 <= own * ratio_bps as u64 {
                    continue;
                }
                if let Some(step) = retreat_step(state, rules, civ, u.hex) {
                    implicit.push((civ, Order::MoveUnit { unit: u.id, path: alloc::vec![step] }));
                }
            }
            StandingRule::Patrol { route, len, next } => {
                if !u.path.is_empty() || len == 0 {
                    continue;
                }
                let mut n = next % len;
                if u.hex == route[n as usize] {
                    n = (n + 1) % len;
                }
                let (id, here) = (u.id, u.hex);
                if let StandingRule::Patrol { next, .. } = &mut state.units[i].standing {
                    *next = n;
                }
                let goal = route[n as usize];
                if goal == here {
                    continue;
                }
                if let Some(mut path) = crate::preview::path_to(state, rules, id, goal) {
                    path.truncate(rules.max_path_len as usize);
                    implicit.push((civ, Order::MoveUnit { unit: id, path }));
                }
            }
        }
    }
    state.implicit = implicit;

    // AutoPurchase: the same effect as a manual `Purchase`, at no order cost.
    for id in 0..state.cities.len() as u32 {
        let c = &state.cities[id as usize];
        let (Some(civ), gold) = (c.owner, c.standing.auto_purchase) else { continue };
        if c.alive && gold > 0 && !c.queue.is_empty() && !manual_purchase.contains(&id) {
            crate::tick::purchase(state, rules, civ, id, gold);
        }
    }
}

/// One step toward the nearest own city: the enterable free neighbour that
/// most reduces that distance (ties: lowest hex). `None` if no step helps.
fn retreat_step(state: &WorldState, rules: &Ruleset, civ: CivId, from: Hex) -> Option<Hex> {
    let home = |h: Hex| state.living_cities_of(civ).map(|c| c.hex.distance(h)).min();
    let now = home(from)?;
    if now == 0 {
        return None; // already inside a city
    }
    from.neighbors()
        .into_iter()
        .filter(|h| {
            crate::tick::may_enter(state, rules, Some(civ), *h) && !state.units.iter().any(|x| x.alive && x.hex == *h)
        })
        .filter_map(|h| home(h).map(|d| (d, h)))
        .filter(|(d, _)| *d < now)
        .min()
        .map(|(_, h)| h)
}
