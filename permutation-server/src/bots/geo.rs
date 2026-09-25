//! Map helpers shared by the planning steps: paths, sites, counts.

use permutation_rules::hex::Hex;
use permutation_rules::state::{CivId, Owner, Unit, WorldState};
use permutation_rules::tick::may_enter;
use permutation_rules::Ruleset;
use std::collections::{BTreeSet, HashMap, VecDeque};

fn passable(s: &WorldState, h: Hex) -> bool {
    s.map.tile(h).is_some_and(|t| t.terrain.is_passable())
}

/// BFS over tiles the civ may enter; returns the path excluding `from`.
pub(super) fn path_to(
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

pub(super) fn my_units(s: &WorldState, civ: CivId) -> impl Iterator<Item = &Unit> {
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

pub(super) fn good_site(s: &WorldState, r: &Ruleset, civ: CivId, h: Hex) -> bool {
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

pub(super) fn site_value(s: &WorldState, h: Hex) -> u32 {
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
