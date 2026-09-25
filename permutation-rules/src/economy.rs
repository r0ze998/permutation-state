//! City and civilization economy formulas (§5, §6).

use crate::buildings::Building;
use crate::fixed::{isqrt, Bps, BPS_ONE};
use crate::map::Map;
use crate::params::Ruleset;
use crate::state::Unit;
use crate::state::{City, Focus};
use crate::tech::{info as tech_info, Tech};
use crate::units::stats;
use alloc::vec::Vec;

/// `G(p) = 15 + 8(p−1) + isqrt((p−1)³)` (§5.3).
pub fn growth_threshold(pop: u32) -> u64 {
    let k = pop.saturating_sub(1) as u64;
    15 + 8 * k + isqrt(k * k * k)
}

/// `T × (20 + T) / 80`, T = whole effective troops, T2 counted ×1.5 (§6.1).
pub fn unit_upkeep<'a>(units: impl Iterator<Item = &'a Unit>) -> u64 {
    upkeep_of_effective(units.map(effective_troops_milli).sum())
}

/// A unit's effective troops for upkeep, in milli-troops (civilians: 0).
pub fn effective_troops_milli(u: &Unit) -> u64 {
    if u.unit_type.is_civilian() {
        0
    } else {
        u.troops as u64 * stats(u.unit_type).upkeep_bps as u64 / BPS_ONE as u64
    }
}

/// Unit upkeep of a civ with `eff_milli` effective milli-troops in total.
pub fn upkeep_of_effective(eff_milli: u64) -> u64 {
    let t = eff_milli / 1000;
    t * (20 + t) / 80
}

/// `(cities − 1)² / 3` (§6.1).
pub fn city_upkeep(cities: u32) -> u64 {
    let k = cities.saturating_sub(1) as u64;
    k * k / 3
}

/// `base × (10000 + per_city × (cities − 1)) / 10000` (§6.2).
pub fn tech_cost(rules: &Ruleset, tech: Tech, cities: u32) -> u64 {
    let base = tech_info(tech).base_cost as u64;
    base * (BPS_ONE as u64 + rules.tech_cost_per_city_bps as u64 * cities.saturating_sub(1) as u64)
        / BPS_ONE as u64
}

/// Star Gate stage cost multiplier from the stalemate breaker (§5.6).
pub fn stalemate_multiplier(rules: &Ruleset, tick: u16) -> Bps {
    if tick < rules.crisis_start {
        return BPS_ONE;
    }
    let step = rules.stalemate_step_bps * (tick - rules.crisis_start) as u32;
    BPS_ONE.saturating_sub(step).max(rules.stalemate_floor_bps)
}

/// Amenities (§5.5).
pub fn amenities(city: &City, civ_cities: u32, war_weariness: u32) -> i32 {
    let temple = if city.buildings.has(Building::Temple) {
        2
    } else {
        0
    };
    3 + temple
        - (city.pop / 3) as i32
        - (civ_cities.saturating_sub(1) / 3) as i32
        - (war_weariness / 20).min(4) as i32
}

/// Order budget per tick: `min(base + cities, cap)` (§4.1).
pub fn order_budget(rules: &Ruleset, cities: u32) -> u16 {
    (rules.budget_base + cities as u16).min(rules.budget_cap)
}

/// Whole-unit yields of one city for one tick, before amenity adjustment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CityYield {
    pub food: u32,
    pub prod: u32,
    pub gold: u32,
    pub science: u32,
    pub influence: u32,
}

fn focus_weights(focus: Focus) -> (u32, u32, u32) {
    match focus {
        Focus::Balanced | Focus::Science => (3, 2, 1),
        Focus::Food => (5, 1, 1),
        Focus::Production => (2, 5, 1),
        Focus::Gold => (2, 1, 5),
    }
}

/// Governor tile assignment and yields (§5.1, §5.2).
///
/// Works the centre plus the best `pop` tiles of the city's territory by
/// `Σ yield × weight`; ties go to the lower tile index.
pub fn city_yield(
    map: &Map,
    city: &City,
    is_capital: bool,
    has_philosophy: bool,
) -> (CityYield, Vec<usize>) {
    let (wf, wp, wg) = focus_weights(city.focus);
    // A city only ever owns tiles within the largest territory radius (§5.4;
    // invariant 11), so only that neighbourhood is scanned.
    let mut worked: Vec<(u32, usize)> = map
        .indices_within(city.hex, crate::map::MAX_TERRITORY_RADIUS)
        .into_iter()
        .map(|i| (i, &map.tiles[i]))
        .filter(|(_, t)| t.owner_city == Some(city.id) && t.hex != city.hex)
        // Water is workable only by an adjacent city (§2.1).
        .filter(|(_, t)| t.terrain.is_land() || t.hex.distance(city.hex) == 1)
        .map(|(i, t)| {
            let (f, p, g) = t.yields();
            (f * wf + p * wp + g * wg, i)
        })
        .collect();
    // Highest score first; lower index wins ties.
    worked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

    let centre = map.tile(city.hex).map(|t| t.yields()).unwrap_or((0, 0, 0));
    let mut y = CityYield {
        food: centre.0.max(2),
        prod: centre.1.max(1),
        gold: centre.2.max(1) + 1,
        science: 1,
        influence: if is_capital { 1 } else { 0 },
    };
    let worked: Vec<usize> = worked
        .into_iter()
        .take(city.pop as usize)
        .map(|(_, i)| i)
        .collect();
    for &i in &worked {
        let (f, p, g) = map.tiles[i].yields();
        y.food += f;
        y.prod += p;
        y.gold += g;
    }
    y.gold += city.pop / 2;
    y.science += city.pop / 3;

    let b = city.buildings;
    if b.has(Building::Granary) {
        y.food += 2;
    }
    if b.has(Building::Workshop) {
        y.prod += 2;
    }
    if b.has(Building::Temple) {
        y.influence += 2 + if has_philosophy { 1 } else { 0 };
    }
    if b.has(Building::Market) {
        y.gold += 3;
        y.gold = y.gold * 12_000 / BPS_ONE;
    }
    if b.has(Building::Academy) {
        let academy = if city.focus == Focus::Science {
            3 * 12_500 / BPS_ONE
        } else {
            3
        };
        y.science += academy;
    }
    if city.heritage_until.is_some() {
        y.food += city.heritage_bonus;
        y.prod += city.heritage_bonus;
    }
    (y, worked)
}

/// Food surplus after consumption and amenity effects (§5.3, §5.5), in whole units.
/// Returns `(surplus, production_bps)`.
pub fn apply_amenities(rules: &Ruleset, food: u32, pop: u32, amenities: i32) -> (i64, Bps) {
    let s = food as i64 - (rules.food_per_pop * pop) as i64;
    match amenities {
        a if a >= 2 && s > 0 => (s * 11_000 / BPS_ONE as i64, BPS_ONE),
        a if a >= 0 => (s, BPS_ONE),
        a if a >= -2 && s > 0 => (s * 5_000 / BPS_ONE as i64, BPS_ONE),
        a if a >= -2 => (s, BPS_ONE),
        _ => (s.min(0), 7_500),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Preset;

    #[test]
    fn growth_threshold_table() {
        let got: Vec<u64> = [1, 2, 3, 4, 5, 6, 8, 10, 12]
            .iter()
            .map(|p| growth_threshold(*p))
            .collect();
        assert_eq!(got, [15, 24, 33, 44, 55, 66, 89, 114, 139]);
    }

    #[test]
    fn city_upkeep_table() {
        let got: Vec<u64> = [2, 4, 6, 8, 10].iter().map(|c| city_upkeep(*c)).collect();
        assert_eq!(got, [0, 3, 8, 16, 27]);
    }

    #[test]
    fn stalemate_breaker_schedule() {
        let r = Ruleset::new(Preset::Blitz);
        assert_eq!(stalemate_multiplier(&r, 119), 10_000);
        assert_eq!(stalemate_multiplier(&r, 120), 10_000);
        assert_eq!(stalemate_multiplier(&r, 130), 9_000);
        assert_eq!(stalemate_multiplier(&r, 179), 6_000);
    }

    #[test]
    fn budget_formula() {
        let r = Ruleset::new(Preset::Season);
        assert_eq!(order_budget(&r, 0), 3); // government in exile
        assert_eq!(order_budget(&r, 1), 4);
        assert_eq!(order_budget(&r, 5), 8);
        assert_eq!(order_budget(&r, 12), 8);
    }

    #[test]
    fn tech_cost_rises_ten_percent_per_city() {
        let r = Ruleset::new(Preset::Blitz);
        assert_eq!(tech_cost(&r, Tech::Astronomy, 1), 260);
        assert_eq!(tech_cost(&r, Tech::Astronomy, 4), 338);
    }
}
