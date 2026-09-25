//! Movement (§7.3): who may enter a tile, occupancy, moving units along
//! their paths in phase 4 in a deterministic order, and the planner
//! (reachable tiles, cheapest paths) that standing rules, previews and
//! agents use — so a planned path is costed exactly as it will be moved.

use crate::checks::Blocked;
use crate::hex::Hex;
use crate::orders::Order;
use crate::params::Ruleset;
use crate::rng::tie_key;
use crate::state::{CivId, Owner, Relation, Unit, WorldState};
use crate::tick::accepted;
use crate::units::{stats, UnitType};
use alloc::collections::{BTreeMap, BinaryHeap};
use alloc::vec::Vec;
use core::cmp::Reverse;

pub(crate) fn allied(state: &WorldState, a: CivId, b: CivId) -> bool {
    a != b && matches!(state.relation(a, b), Relation::Alliance { .. })
}

/// Whether `mover` may enter `hex` (terrain, protected zones, borders). Public
/// so clients and agents can list legal moves with the engine's own rule.
pub fn may_enter(state: &WorldState, rules: &Ruleset, mover: Option<CivId>, hex: Hex) -> bool {
    crate::checks::enter(state, rules, mover, hex).is_ok()
}

/// Occupancy: one army or one civilian per tile; inside an own city, one of each (§7.2).
pub(crate) fn tile_free_for(state: &WorldState, unit: &Unit, hex: Hex) -> bool {
    let occupants = state.units.iter().filter(|u| u.alive && u.hex == hex);
    let own_city = state
        .cities
        .iter()
        .any(|c| c.alive && c.hex == hex && c.owner.is_some() && c.owner == unit.owner.civ());
    free_among(unit, occupants, own_city)
}

/// The occupancy rule, given the living units on the tile and whether it
/// holds a living city of the unit's own civ.
fn free_among<'a>(
    unit: &Unit,
    mut occupants: impl Iterator<Item = &'a Unit>,
    own_city: bool,
) -> bool {
    let is_civ = unit.unit_type.is_civilian();
    occupants.all(|o| {
        o.id == unit.id
            || (o.owner == unit.owner && own_city && o.unit_type.is_civilian() != is_civ)
    })
}

/// Who stands where while phase 4 moves units: for each tile, a linked list
/// of the living units on it (kept up to date as they move), and the living
/// cities with their owners (cities do not change in phase 4). It answers
/// exactly what `tile_free_for` answers, without scanning every unit and
/// city per step; one flat array per tile keeps building it cheap on chain.
struct Occupancy {
    /// First unit on each tile, or `NONE`.
    head: Vec<u32>,
    /// Next unit on the same tile, per unit.
    next: Vec<u32>,
    /// `(tile, owner)` of every living, owned city.
    cities: Vec<(usize, CivId)>,
}

const NONE: u32 = u32::MAX;

impl Occupancy {
    fn new(state: &WorldState) -> Occupancy {
        let mut o = Occupancy {
            head: alloc::vec![NONE; state.map.tiles.len()],
            next: alloc::vec![NONE; state.units.len()],
            cities: Vec::new(),
        };
        for (i, u) in state.units.iter().enumerate().filter(|(_, u)| u.alive) {
            if let Some(t) = state.map.index_of(u.hex) {
                o.next[i] = o.head[t];
                o.head[t] = i as u32;
            }
        }
        for c in state.cities.iter().filter(|c| c.alive) {
            if let (Some(t), Some(owner)) = (state.map.index_of(c.hex), c.owner) {
                o.cities.push((t, owner));
            }
        }
        o
    }

    fn on_tile(&self, t: usize) -> impl Iterator<Item = usize> + '_ {
        let first = Some(self.head[t]).filter(|&i| i != NONE);
        core::iter::successors(first, |&i| {
            Some(self.next[i as usize]).filter(|&n| n != NONE)
        })
        .map(|i| i as usize)
    }

    fn free_for(&self, state: &WorldState, i: usize, hex: Hex) -> bool {
        let unit = &state.units[i];
        let Some(t) = state.map.index_of(hex) else {
            return tile_free_for(state, unit, hex);
        };
        let own_city = unit
            .owner
            .civ()
            .is_some_and(|c| self.cities.iter().any(|&(ct, owner)| ct == t && owner == c));
        free_among(unit, self.on_tile(t).map(|j| &state.units[j]), own_city)
    }

    fn moved(&mut self, state: &WorldState, i: usize, from: Hex, to: Hex) {
        if let Some(t) = state.map.index_of(from) {
            if self.head[t] == i as u32 {
                self.head[t] = self.next[i];
            } else {
                let prev = self.on_tile(t).find(|&j| self.next[j] == i as u32);
                if let Some(prev) = prev {
                    self.next[prev] = self.next[i];
                }
            }
        }
        if let Some(t) = state.map.index_of(to) {
            self.next[i] = self.head[t];
            self.head[t] = i as u32;
        }
    }
}

/// Office and position of an order (see `CivOrders::origin`).
type Origin = (u8, u16);

pub(crate) fn phase_movement(state: &mut WorldState, rules: &Ruleset) {
    // 1. Accept MoveUnit orders (paths continue on later ticks without orders),
    //    manual first, then those compiled from standing rules (§13).
    let mut all: Vec<(CivId, Order, Option<Origin>)> = accepted(state, |o| {
        matches!(o, Order::MoveUnit { .. } | Order::Attack { .. })
    })
    .into_iter()
    .map(|(civ, o, _, g)| (civ, o, Some(g)))
    .collect();
    all.extend(state.implicit.iter().cloned().map(|(c, o)| (c, o, None)));
    {
        for (civ, order, origin) in all {
            if let Order::MoveUnit { unit, path } = order {
                let Some(u) = state
                    .units
                    .get(unit as usize)
                    .filter(|u| u.alive && u.owner == Owner::Civ(civ))
                else {
                    if let Some(g) = origin {
                        state.skip(civ, g, Blocked::UnknownUnit.code());
                    }
                    continue;
                };
                let mut prev = u.hex;
                let valid = !path.is_empty()
                    && path.iter().all(|h| {
                        let ok = prev.distance(*h) == 1
                            && state.map.tile(*h).is_some_and(|t| t.terrain.is_passable());
                        prev = *h;
                        ok
                    });
                if valid {
                    state.units[unit as usize].path = path;
                } else if let Some(g) = origin {
                    state.skip(civ, g, Blocked::Impassable.code());
                }
            } else if let Order::Attack { army, .. } = order {
                // An attacking army holds its position this tick (§8, v0.1 clarification).
                if let Some(u) = state
                    .units
                    .get_mut(army as usize)
                    .filter(|u| u.alive && u.owner == Owner::Civ(civ))
                {
                    u.path.clear();
                }
            }
        }
    }

    crate::probe::probe("cu p4.accept");
    // 2. Sub-steps (§15 phase 4).
    let full_mp: Vec<u8> = state
        .units
        .iter()
        .map(|u| stats(u.unit_type).movement)
        .collect();
    let mut mp_left = full_mp.clone();
    let mut moved = alloc::vec![false; state.units.len()];
    let max_mp = full_mp.iter().copied().max().unwrap_or(0);
    let mut occupancy = Occupancy::new(state);
    crate::probe::probe("cu p4.setup");
    for _ in 0..max_mp {
        let mut intents: Vec<(u64, usize, Hex, u8)> = Vec::new();
        for (i, u) in state.units.iter().enumerate() {
            if !u.alive || mp_left[i] == 0 || u.path.is_empty() {
                continue;
            }
            let next = u.path[0];
            let cost = move_cost(state, u.unit_type, next);
            // A unit with full MP may always move one tile (§7.3).
            if cost == 0 || (mp_left[i] < cost && mp_left[i] != full_mp[i]) {
                mp_left[i] = 0;
                continue;
            }
            intents.push((tie_key(&state.tick_seed, u.id as u64), i, next, cost));
        }
        crate::probe::probe("cu p4.intents");
        if intents.is_empty() {
            break;
        }
        intents.sort();
        for (_, i, next, cost) in intents {
            if !may_enter(state, rules, state.units[i].owner.civ(), next) {
                state.units[i].path.clear();
                mp_left[i] = 0;
                continue;
            }
            if !occupancy.free_for(state, i, next) {
                continue; // wait this sub-step; never swap (§15 phase 4)
            }
            occupancy.moved(state, i, state.units[i].hex, next);
            let u = &mut state.units[i];
            u.hex = next;
            u.path.remove(0);
            mp_left[i] = mp_left[i].saturating_sub(cost);
            moved[i] = true;
        }
    }
    let tick = state.tick;
    for (i, u) in state.units.iter_mut().enumerate() {
        if moved[i] {
            u.last_moved = Some(tick);
        }
        u.used_full_mp = moved[i] && mp_left[i] == 0;
    }
}

// ------------------------------------------------------------------ planning

/// A tile a unit can reach, with the movement cost and ticks needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reach {
    pub hex: Hex,
    pub cost: u32,
    pub ticks: u32,
    pub steps: u32,
}

/// Movement points a unit of type `unit` spends to enter `h` (§7.3): the
/// terrain's cost, or 1 for a scout. 0 outside the map. Used by phase 4.
pub fn move_cost(state: &WorldState, unit: UnitType, h: Hex) -> u8 {
    if unit == UnitType::Scout {
        1
    } else {
        state.map.tile(h).map_or(0, |t| t.terrain.info().move_cost)
    }
}

/// The cost of a step for planning: `None` when the tile cannot be entered.
fn step_cost(state: &WorldState, unit: UnitType, h: Hex) -> Option<u32> {
    let t = state.map.tile(h)?;
    t.terrain
        .is_passable()
        .then(|| move_cost(state, unit, h) as u32)
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
    // Ties between equal paths are broken by hexes seen from the unit's
    // sextant, and neighbours are tried in the order turned to it, so the
    // chosen path turns with a symmetric map.
    let k = u.hex.sextant();
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u32, 0u32, u.hex.turned(k), u.hex)));
    best.insert(
        u.hex,
        Reach {
            hex: u.hex,
            cost: 0,
            ticks: 0,
            steps: 0,
        },
    );
    while let Some(Reverse((cost, steps, _, h))) = heap.pop() {
        if best.get(&h).is_some_and(|r| r.cost < cost) || steps >= rules.max_path_len as u32 {
            continue;
        }
        for n in h.neighbors_in(k) {
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
                heap.push(Reverse((nc, steps + 1, n.turned(k), n)));
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

#[cfg(test)]
mod occupancy_tests {
    use super::*;
    use crate::genesis::{nation_entries, new_season};
    use crate::params::Preset;

    /// The index answers exactly what the full scan answers, for every unit
    /// and every neighbouring tile, also after units move around.
    #[test]
    fn occupancy_agrees_with_the_full_scan() {
        let rules = Ruleset::new(Preset::Blitz);
        let mut s = new_season(&rules, &[3; 32], &[4; 32], &nation_entries(6)).unwrap();
        // Crowd a few tiles: every unit steps onto its first passable neighbour.
        let mut occ = Occupancy::new(&s);
        let check = |s: &WorldState, occ: &Occupancy| {
            for i in 0..s.units.len() {
                for h in s.units[i]
                    .hex
                    .neighbors()
                    .into_iter()
                    .chain([s.units[i].hex])
                {
                    assert_eq!(
                        occ.free_for(s, i, h),
                        tile_free_for(s, &s.units[i], h),
                        "unit {i} to {h:?}"
                    );
                }
            }
        };
        check(&s, &occ);
        for round in 0..3 {
            for i in 0..s.units.len() {
                let from = s.units[i].hex;
                let Some(to) = from
                    .neighbors()
                    .into_iter()
                    .cycle()
                    .skip(round)
                    .take(6)
                    .find(|h| s.map.tile(*h).is_some_and(|t| t.terrain.is_passable()))
                else {
                    continue;
                };
                occ.moved(&s, i, from, to);
                s.units[i].hex = to;
                check(&s, &occ);
            }
        }
    }
}
