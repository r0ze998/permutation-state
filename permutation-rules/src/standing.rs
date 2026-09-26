//! Standing rules (§13): first-party automation, identical for humans and agents.
//!
//! `SetStanding` costs one order and is applied in phase 2. In phase 3 every
//! rule is compiled into *implicit orders* (`WorldState::implicit`) that
//! phases 4 (movement) and 5 (combat) treat exactly like manual ones.
//! Execution is free. A unit that received a manual order this tick skips
//! its rule for that tick.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use crate::checks::Blocked;
use crate::hex::Hex;
use crate::movement::Occupancy;
use crate::orders::{AttackTarget, Order, StandingOrder, StandingTarget};
use crate::params::Ruleset;
use crate::state::{CityStanding, CivId, Owner, StandingRule, WorldState};
use crate::units::stats;

/// Phase 2: apply a validated `SetStanding`.
pub(crate) fn apply(
    state: &mut WorldState,
    civ: CivId,
    target: StandingTarget,
    rule: StandingOrder,
) -> Result<(), Blocked> {
    // Invalid at resolution: dropped without refund (§4.1).
    crate::checks::standing(state, civ, target, &rule)?;
    match target {
        StandingTarget::Unit(id) => {
            let u = &mut state.units[id as usize];
            u.standing = match rule {
                StandingOrder::AutoDefend { radius } => StandingRule::AutoDefend {
                    radius,
                    anchor: u.hex,
                },
                StandingOrder::Retreat { ratio_bps } => StandingRule::Retreat { ratio_bps },
                StandingOrder::Patrol { route } => {
                    let mut r = [u.hex; 6];
                    r[..route.len()].copy_from_slice(&route);
                    StandingRule::Patrol {
                        route: r,
                        len: route.len() as u8,
                        next: 0,
                    }
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
    Ok(())
}

/// Combat strength of a living army: troops × unit strength.
fn strength(state: &WorldState, id: usize) -> u64 {
    let u = &state.units[id];
    u.troops as u64 * stats(u.unit_type).strength as u64
}

/// Phase 3: compile every rule into implicit orders (and apply AutoPurchase).
///
/// Bounded work (on chain the whole phase runs in one transaction): each
/// unit rule looks only at the tiles around its unit, through one
/// occupancy index built for the phase, and a patrol plans at most one
/// tick of movement without searching (`patrol_path`).
pub(crate) fn phase_standing(state: &mut WorldState, rules: &Ruleset) {
    crate::probe::probe("cu p3.start");
    state.implicit.clear();
    let mut manual: BTreeSet<u32> = BTreeSet::new();
    let mut manual_purchase: BTreeSet<u32> = BTreeSet::new();
    for a in &state.tick_orders {
        for o in &a.orders {
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
    let occ = state
        .units
        .iter()
        .any(|u| u.alive && u.standing != StandingRule::None)
        .then(|| Occupancy::new(state));
    crate::probe::probe("cu p3.occ");
    // Rules run for at most `ruled_units_cap` units per nation (lowest ids).
    let cap = crate::orders::ruled_units_cap(state.civs.len());
    let mut running = alloc::vec![0usize; state.civs.len()];
    for i in 0..state.units.len() {
        let u = &state.units[i];
        let Owner::Civ(civ) = u.owner else { continue };
        if !u.alive || u.standing == StandingRule::None {
            continue;
        }
        let Some(slot) = running.get_mut(civ as usize) else {
            continue;
        };
        if *slot >= cap {
            continue; // waits until a lower-id rule is cleared, dies or is captured
        }
        *slot += 1;
        if manual.contains(&u.id) {
            continue;
        }
        let Some(occ) = occ.as_ref() else { continue };
        match u.standing {
            StandingRule::None => {}
            StandingRule::AutoDefend { radius, anchor } => {
                let reach = stats(u.unit_type).range.max(1) as u32;
                // The weakest hostile army in reach and near the anchor
                // (ties: lowest id), from the tiles in reach only.
                let mut target: Option<(u32, u32)> = None;
                state.map.for_each_within(u.hex, reach, |t| {
                    if state.map.tiles[t].hex.distance(anchor) > radius as u32 {
                        return;
                    }
                    for j in occ.on_tile(t) {
                        let e = &state.units[j];
                        if !e.unit_type.is_civilian()
                            && crate::battle::hostile(state, civ, e.owner)
                            && target.is_none_or(|b| (e.troops, e.id) < b)
                        {
                            target = Some((e.troops, e.id));
                        }
                    }
                });
                if let Some((_, e)) = target {
                    implicit.push((
                        civ,
                        Order::Attack {
                            army: u.id,
                            target: AttackTarget::Unit(e),
                        },
                    ));
                }
            }
            StandingRule::Retreat { ratio_bps } => {
                let own = strength(state, i);
                let enemy: u64 = u
                    .hex
                    .neighbors()
                    .into_iter()
                    .flat_map(|h| occ.at(state, h))
                    .filter(|&j| {
                        let e = &state.units[j];
                        !e.unit_type.is_civilian() && crate::battle::hostile(state, civ, e.owner)
                    })
                    .map(|j| strength(state, j))
                    .sum();
                if own == 0 || enemy * crate::fixed::BPS_ONE as u64 <= own * ratio_bps as u64 {
                    continue;
                }
                if let Some(step) = retreat_step(state, rules, occ, civ, u.hex) {
                    implicit.push((
                        civ,
                        Order::MoveUnit {
                            unit: u.id,
                            path: alloc::vec![step],
                        },
                    ));
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
                let goal = route[n as usize];
                let (id, here) = (u.id, u.hex);
                let path = if goal == here {
                    Vec::new()
                } else {
                    patrol_path(state, rules, occ, i, goal)
                };
                if goal != here && path.is_empty() {
                    // No step toward this waypoint: head for the next one.
                    n = (n + 1) % len;
                }
                if let StandingRule::Patrol { next, .. } = &mut state.units[i].standing {
                    *next = n;
                }
                if !path.is_empty() {
                    implicit.push((civ, Order::MoveUnit { unit: id, path }));
                }
            }
        }
    }
    state.implicit = implicit;
    crate::probe::probe("cu p3.units");

    // AutoPurchase: the same effect as a manual `Purchase`, at no order cost.
    for id in 0..state.cities.len() as u32 {
        let c = &state.cities[id as usize];
        let (Some(civ), gold) = (c.owner, c.standing.auto_purchase) else {
            continue;
        };
        if c.alive && gold > 0 && !c.queue.is_empty() && !manual_purchase.contains(&id) {
            // Same rules as a manual purchase: never a Star Gate stage (v0.2 C3).
            let _ = crate::tick::purchase(state, rules, civ, id, gold);
        }
    }
    crate::probe::probe("cu p3.end");
}

/// A patrol's steps for this tick toward `goal`, without a search: at most
/// the unit's movement points of steps (and `max_path_len`), each onto one
/// of the (one or two) neighbours one tile closer to the goal, the one
/// nearest the straight line from the unit to the goal first (ties: lowest
/// hex seen from the unit's sextant, so the walk turns with a symmetric
/// map). A step must be passable, enterable (`may_enter`) and free of
/// foreign units; the walk stops at the first tile where neither
/// neighbour is. Empty when not even one step is possible. Cost: at most
/// 3 steps × 2 candidates, each one `may_enter`.
pub(crate) fn patrol_path(
    state: &WorldState,
    rules: &Ruleset,
    occ: &Occupancy,
    unit: usize,
    goal: Hex,
) -> Vec<Hex> {
    let u = &state.units[unit];
    let mover = u.owner.civ();
    let steps = (stats(u.unit_type).movement.max(1)).min(rules.max_path_len) as usize;
    let k = u.hex.sextant();
    let (start, gq, gr) = (u.hex, goal.q - u.hex.q, goal.r - u.hex.r);
    // |cross product| with the start→goal vector: how far a hex strays
    // from the straight line (a 60° turn keeps it, so it is fair on a
    // symmetric map).
    let off_line = |h: Hex| {
        ((h.q - start.q) as i64 * gr as i64 - (h.r - start.r) as i64 * gq as i64).unsigned_abs()
    };
    let mut path = Vec::new();
    let mut cur = start;
    while path.len() < steps && cur != goal {
        let d = cur.distance(goal);
        let mut closer: [Option<(u64, Hex, Hex)>; 2] = [None, None];
        for n in cur.neighbors_in(k) {
            if n.distance(goal) < d {
                let key = Some((off_line(n), n.turned(k), n));
                if closer[0].is_none() {
                    closer[0] = key;
                } else {
                    closer[1] = key;
                }
            }
        }
        if closer[1] < closer[0] && closer[1].is_some() {
            closer.swap(0, 1);
        }
        let step = closer.into_iter().flatten().map(|(_, _, h)| h).find(|&h| {
            let Some(t) = state.map.index_of(h) else {
                return false;
            };
            state.map.tiles[t].terrain.is_passable()
                && !occ.on_tile(t).any(|j| state.units[j].owner != u.owner)
                && crate::tick::may_enter(state, rules, mover, h)
        });
        let Some(h) = step else { break };
        path.push(h);
        cur = h;
    }
    path
}

/// One step toward the nearest own city: the enterable free neighbour that
/// most reduces that distance (ties: lowest hex as seen from the unit's
/// sextant, so the choice turns with a symmetric map). `None` if no step
/// helps.
fn retreat_step(
    state: &WorldState,
    rules: &Ruleset,
    occ: &Occupancy,
    civ: CivId,
    from: Hex,
) -> Option<Hex> {
    let home = |h: Hex| state.living_cities_of(civ).map(|c| c.hex.distance(h)).min();
    let now = home(from)?;
    if now == 0 {
        return None; // already inside a city
    }
    let k = from.sextant();
    from.neighbors()
        .into_iter()
        .filter(|h| {
            // The cheap test first; both are pure, so the order does not matter.
            occ.at(state, *h).next().is_none()
                && crate::tick::may_enter(state, rules, Some(civ), *h)
        })
        .filter_map(|h| home(h).map(|d| (d, h.turned(k), h)))
        .filter(|(d, _, _)| *d < now)
        .min()
        .map(|(_, _, h)| h)
}

#[cfg(test)]
mod bounded_tests {
    use super::*;
    use crate::genesis::{nation_entries, new_season};
    use crate::params::Preset;
    use crate::state::{Relation, Unit};
    use crate::units::UnitType;

    fn world() -> (Ruleset, WorldState) {
        let rules = Ruleset::new(Preset::Blitz);
        let s = new_season(&rules, &[5; 32], &[6; 32], &nation_entries(6)).unwrap();
        (rules, s)
    }

    fn at_war(s: &mut WorldState, a: CivId, b: CivId) {
        let war = Relation::War {
            declared_by: a,
            casus_belli: false,
            active_from: 0,
            peace_at: None,
        };
        s.set_relation(a, b, war);
    }

    fn unit(id: u32, owner: Owner, unit_type: UnitType, troops: u32, hex: Hex) -> Unit {
        Unit {
            id,
            owner,
            unit_type,
            troops,
            hex,
            path: Vec::new(),
            last_moved: None,
            used_full_mp: false,
            standing: StandingRule::None,
            alive: true,
        }
    }

    /// Passable tiles without a unit or a city, in tile order.
    fn free_tiles(s: &WorldState) -> Vec<Hex> {
        let taken: BTreeSet<Hex> = s
            .units
            .iter()
            .map(|u| u.hex)
            .chain(s.cities.iter().map(|c| c.hex))
            .collect();
        s.map
            .tiles
            .iter()
            .filter(|t| t.terrain.is_passable() && !taken.contains(&t.hex))
            .map(|t| t.hex)
            .collect()
    }

    /// Every nation at war with every other (no capital protection), plus up to 300 units on every
    /// `stride`-th free tile (every 11th dead, every third a scout without a
    /// rule); the others get `rule(i, free tiles)`.
    fn crowded(
        stride: usize,
        rule: impl Fn(usize, &[Hex]) -> StandingRule,
    ) -> (Ruleset, WorldState) {
        let (rules, mut s) = world();
        let n = s.civs.len() as CivId;
        for a in 0..n {
            for b in a + 1..n {
                at_war(&mut s, a, b);
            }
        }
        for c in &mut s.civs {
            c.protection_lost = true;
        }
        let free = free_tiles(&s);
        for (i, h) in free.iter().step_by(stride).take(300).enumerate() {
            let ty = [UnitType::Archer, UnitType::Spearman, UnitType::Scout][i % 3];
            let owner = Owner::Civ((i / 3 % 6) as CivId);
            let troops = 500 + (i as u32 * 37) % 19_000;
            let mut u = unit(s.units.len() as u32, owner, ty, troops, *h);
            if ty != UnitType::Scout {
                u.standing = rule(i, &free);
            }
            u.alive = i % 11 != 0;
            s.units.push(u);
        }
        s.tick_orders.clear();
        (rules, s)
    }

    /// The living ruled units whose rules run: the first `ruled_units_cap`
    /// of each nation, by id.
    fn running(s: &WorldState) -> Vec<usize> {
        let cap = crate::orders::ruled_units_cap(s.civs.len());
        let mut seen = alloc::vec![0usize; s.civs.len()];
        (0..s.units.len())
            .filter(|&i| {
                let u = &s.units[i];
                let Owner::Civ(c) = u.owner else { return false };
                if !u.alive || u.standing == StandingRule::None {
                    return false;
                }
                seen[c as usize] += 1;
                seen[c as usize] <= cap
            })
            .collect()
    }

    /// The old full scan of AutoDefend, kept as the reference.
    fn old_auto_defend(
        state: &WorldState,
        u: &Unit,
        civ: CivId,
        radius: u8,
        anchor: Hex,
    ) -> Option<u32> {
        let reach = stats(u.unit_type).range.max(1) as u32;
        state
            .units
            .iter()
            .filter(|e| {
                e.alive
                    && !e.unit_type.is_civilian()
                    && crate::battle::hostile(state, civ, e.owner)
                    && e.hex.distance(u.hex) <= reach
                    && e.hex.distance(anchor) <= radius as u32
            })
            .min_by_key(|e| (e.troops, e.id))
            .map(|e| e.id)
    }

    /// The old full scans of Retreat (enemy strength and the step home),
    /// kept as the reference.
    fn old_retreat(
        state: &WorldState,
        rules: &Ruleset,
        i: usize,
        civ: CivId,
        ratio_bps: u32,
    ) -> Option<Hex> {
        let u = &state.units[i];
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
        if own == 0 || enemy * crate::fixed::BPS_ONE as u64 <= own * ratio_bps as u64 {
            return None;
        }
        let home = |h: Hex| state.living_cities_of(civ).map(|c| c.hex.distance(h)).min();
        let now = home(u.hex)?;
        if now == 0 {
            return None;
        }
        let k = u.hex.sextant();
        u.hex
            .neighbors()
            .into_iter()
            .filter(|h| {
                crate::tick::may_enter(state, rules, Some(civ), *h)
                    && !state.units.iter().any(|x| x.alive && x.hex == *h)
            })
            .filter_map(|h| home(h).map(|d| (d, h.turned(k), h)))
            .filter(|(d, _, _)| *d < now)
            .min()
            .map(|(_, _, h)| h)
    }

    /// Patrol plans turn with the world: on a symmetric map, turning the
    /// world by k × 60° turns every plan by the same angle.
    #[test]
    fn patrol_paths_turn_with_the_world() {
        let (rules, s) = world();
        let goals: Vec<Hex> = s.map.tiles.iter().step_by(7).map(|t| t.hex).collect();
        for k in 1..6u8 {
            let mut t = s.clone();
            crate::mapgen::rotate_world(&mut t, k);
            let (o1, o2) = (Occupancy::new(&s), Occupancy::new(&t));
            let mut moved = 0;
            for i in 0..s.units.len() {
                for g in &goals {
                    let a = patrol_path(&s, &rules, &o1, i, *g);
                    let b = patrol_path(&t, &rules, &o2, i, g.rotate_by(k));
                    let turned: Vec<Hex> = a.iter().map(|h| h.rotate_by(k)).collect();
                    assert_eq!(turned, b, "unit {i} goal {g:?} k {k}");
                    moved += !a.is_empty() as usize;
                }
            }
            assert!(moved > 0);
        }
    }

    /// A plan never exceeds the unit's movement points, and every step is
    /// one tile closer to the goal, passable, enterable and free of foreign
    /// units.
    #[test]
    fn patrol_paths_are_short_and_legal() {
        let (rules, s) = world();
        let occ = Occupancy::new(&s);
        for i in 0..s.units.len() {
            let u = &s.units[i];
            for t in s.map.tiles.iter().step_by(5) {
                let p = patrol_path(&s, &rules, &occ, i, t.hex);
                assert!(p.len() <= stats(u.unit_type).movement.max(1) as usize);
                let mut prev = u.hex;
                for h in &p {
                    assert_eq!(prev.distance(*h), 1);
                    assert_eq!(h.distance(t.hex) + 1, prev.distance(t.hex));
                    assert!(s.map.tile(*h).unwrap().terrain.is_passable());
                    assert!(crate::tick::may_enter(&s, &rules, u.owner.civ(), *h));
                    assert!(s
                        .units
                        .iter()
                        .all(|x| !x.alive || x.hex != *h || x.owner == u.owner));
                    prev = *h;
                }
            }
        }
    }

    /// The indexed AutoDefend picks exactly what the full scan picked, in a
    /// crowded world at war.
    #[test]
    fn indexed_auto_defend_matches_the_scan() {
        let (rules, mut s) = crowded(1, |i, free| StandingRule::AutoDefend {
            radius: 1 + (i % 3) as u8,
            anchor: free[i.saturating_sub(i % 2)],
        });
        let expected: Vec<(u32, u32)> = running(&s)
            .into_iter()
            .filter_map(|i| {
                let u = &s.units[i];
                match (u.standing, u.owner) {
                    (StandingRule::AutoDefend { radius, anchor }, Owner::Civ(c)) => {
                        old_auto_defend(&s, u, c, radius, anchor).map(|t| (u.id, t))
                    }
                    _ => None,
                }
            })
            .collect();
        phase_standing(&mut s, &rules);
        let got: Vec<(u32, u32)> = s
            .implicit
            .iter()
            .filter_map(|(_, o)| match o {
                Order::Attack {
                    army,
                    target: AttackTarget::Unit(t),
                } => Some((*army, *t)),
                _ => None,
            })
            .collect();
        assert!(
            expected.len() > 10,
            "too few rules fire: {}",
            expected.len()
        );
        assert_eq!(got, expected);
    }

    /// The indexed Retreat (enemy strength from the six neighbours, the
    /// free-tile test from the index) steps exactly where the full scans
    /// stepped, in a crowded world at war.
    #[test]
    fn indexed_retreat_matches_the_scan() {
        let (rules, mut s) = crowded(2, |i, _| StandingRule::Retreat {
            ratio_bps: [1_000, 10_000, 30_000][i % 3],
        });
        let expected: Vec<(u32, Hex)> = running(&s)
            .into_iter()
            .filter_map(|i| {
                let u = &s.units[i];
                match (u.standing, u.owner) {
                    (StandingRule::Retreat { ratio_bps }, Owner::Civ(c)) => {
                        old_retreat(&s, &rules, i, c, ratio_bps).map(|h| (u.id, h))
                    }
                    _ => None,
                }
            })
            .collect();
        phase_standing(&mut s, &rules);
        let got: Vec<(u32, Hex)> = s
            .implicit
            .iter()
            .filter_map(|(_, o)| match o {
                Order::MoveUnit { unit, path } if path.len() == 1 => Some((*unit, path[0])),
                _ => None,
            })
            .collect();
        assert!(
            expected.len() > 10,
            "too few rules fire: {}",
            expected.len()
        );
        assert_eq!(got, expected);
    }

    /// Only the first `ruled_units_cap` ruled units of a nation run.
    #[test]
    fn at_most_the_cap_of_rules_run_per_nation() {
        let (rules, mut s) = world();
        at_war(&mut s, 0, 1);
        let free = free_tiles(&s);
        let cap = crate::orders::ruled_units_cap(s.civs.len());
        // cap + 2 archers of nation 0, each next to a weak army of nation 1.
        let mut placed = BTreeSet::new();
        let mut ours = Vec::new();
        for h in &free {
            if ours.len() == cap + 2 {
                break;
            }
            if placed.contains(h) {
                continue;
            }
            let Some(n) = h
                .neighbors()
                .into_iter()
                .find(|n| free.contains(n) && !placed.contains(n))
            else {
                continue;
            };
            placed.insert(*h);
            placed.insert(n);
            let id = s.units.len() as u32;
            let mut archer = unit(id, Owner::Civ(0), UnitType::Archer, 10_000, *h);
            archer.standing = StandingRule::AutoDefend {
                radius: 1,
                anchor: *h,
            };
            s.units.push(archer);
            s.units
                .push(unit(id + 1, Owner::Civ(1), UnitType::Spearman, 1_000, n));
            ours.push(id);
        }
        let attackers = |s: &WorldState| -> Vec<u32> {
            s.implicit
                .iter()
                .filter_map(|(_, o)| match o {
                    Order::Attack { army, .. } => Some(*army),
                    _ => None,
                })
                .collect()
        };
        s.tick_orders.clear();
        phase_standing(&mut s, &rules);
        assert_eq!(attackers(&s), ours[..cap].to_vec());
        // A manual order skips the lowest one's rule for the tick, but it
        // still holds its place: the one past the cap stays dormant.
        s.tick_orders.push(crate::orders::CivOrders {
            civ: 0,
            orders: alloc::vec![Order::MoveUnit {
                unit: ours[0],
                path: Vec::new(),
            }],
            credits: Vec::new(),
            origin: Vec::new(),
            spent: [0; 4],
        });
        phase_standing(&mut s, &rules);
        assert_eq!(attackers(&s), ours[1..cap].to_vec());
        s.tick_orders.clear();
        // The lowest one dies: the next one takes its place.
        s.units[ours[0] as usize].alive = false;
        phase_standing(&mut s, &rules);
        assert_eq!(attackers(&s), ours[1..cap + 1].to_vec());
    }
}
