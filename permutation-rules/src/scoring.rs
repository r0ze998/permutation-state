//! Achievements (Game Design V5 §6): four paths, five milestone tiers each,
//! and eras.
//!
//! Every milestone is an absolute threshold, so any number of nations can
//! reach it. A path's tier is the highest `k` such that milestones `1..=k`
//! all hold (a ladder). Milestones built only on quantities that never
//! decrease (wealth, techs, the Star Gate record, trade, "was suzerain",
//! "sent an envoy") are permanent once reached; the others (territory,
//! population, suzerainties, treaties, captured cities held) are judged on
//! the state at the end of the season and shown provisionally until then.

use crate::params::Ruleset;
use crate::state::{CivId, Relation, WorldState};

pub const PATH_NAMES: [&str; 4] = ["hegemony", "prosperity", "science", "concord"];

/// Tiles owned by the civ's living cities.
pub fn owned_tiles(state: &WorldState, civ: CivId) -> u32 {
    state
        .map
        .tiles
        .iter()
        .filter(|t| {
            t.owner_city
                .and_then(|id| state.cities.get(id as usize))
                .is_some_and(|c| c.alive && c.owner == Some(civ))
        })
        .count() as u32
}

/// Cities held that the civ took from another founder (V5 §6.2). The city
/// must have been at least `capture_min_founded_age` ticks old when taken.
pub fn captured_held(state: &WorldState, civ: CivId) -> u32 {
    state
        .living_cities_of(civ)
        .filter(|c| c.captured_tick.is_some() && c.capture_scores && c.founder != civ)
        .count() as u32
}

pub fn total_pop(state: &WorldState, civ: CivId) -> u32 {
    state.living_cities_of(civ).map(|c| c.pop).sum()
}

/// Civs with a NAP or an alliance with `civ` ("条約相手").
pub fn treaty_partners(state: &WorldState, civ: CivId) -> u32 {
    (0..state.civs.len() as CivId)
        .filter(|o| *o != civ && matches!(state.relation(civ, *o), Relation::Nap { .. } | Relation::Alliance { .. }))
        .count() as u32
}

pub fn alliances(state: &WorldState, civ: CivId) -> u32 {
    (0..state.civs.len() as CivId)
        .filter(|o| *o != civ && matches!(state.relation(civ, *o), Relation::Alliance { .. }))
        .count() as u32
}

pub fn suzerainties(state: &WorldState, civ: CivId) -> u32 {
    state
        .city_states
        .iter()
        .filter(|cs| cs.suzerain == Some(civ) && cs.captured_by.is_none())
        .count() as u32
}

/// Trade volume with each counterparty counted up to
/// `trade_counterparty_bps` of the raw total (V5 §6.2 "馴れ合いの防止").
pub fn trade_effective(rules: &Ruleset, trade: &[u64]) -> u64 {
    let raw: u64 = trade.iter().sum();
    let cap = raw as u128 * rules.trade_counterparty_bps as u128 / 10_000;
    trade.iter().map(|v| (*v as u128).min(cap) as u64).sum()
}

/// Everything the milestones read, for one civ at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub tiles: u32,
    pub captured_held: u32,
    pub pop: u32,
    pub wealth: u64,
    pub techs: u32,
    pub star_gate_max: u8,
    pub partners: u32,
    pub alliances: u32,
    pub suzerainties: u32,
    pub ever_suzerain: bool,
    pub envoy_sent: bool,
    pub trade: u64,
}

/// Facts for every civ, with one pass over the map.
pub fn facts_all(state: &WorldState, rules: &Ruleset) -> alloc::vec::Vec<Facts> {
    let n = state.civs.len();
    let mut tiles = alloc::vec![0u32; n];
    for t in &state.map.tiles {
        if let Some(owner) = t.owner_city.and_then(|id| state.cities.get(id as usize)).filter(|c| c.alive).and_then(|c| c.owner) {
            tiles[owner as usize] += 1;
        }
    }
    (0..n as CivId)
        .map(|civ| {
            let c = &state.civs[civ as usize];
            let a = &c.achievements;
            Facts {
                tiles: tiles[civ as usize],
                captured_held: captured_held(state, civ),
                pop: total_pop(state, civ),
                wealth: a.wealth,
                techs: c.techs.count(),
                star_gate_max: a.star_gate_max,
                partners: treaty_partners(state, civ),
                alliances: alliances(state, civ),
                suzerainties: suzerainties(state, civ),
                ever_suzerain: a.ever_suzerain,
                envoy_sent: a.envoy_sent,
                trade: trade_effective(rules, &a.trade),
            }
        })
        .collect()
}

/// Whether milestone `tier` (1..=5) of `path` (0..4) holds (V5 §6.2).
pub fn milestone(rules: &Ruleset, f: &Facts, path: usize, tier: usize) -> bool {
    let k = tier - 1;
    match path {
        0 => {
            let tiles_ok = f.tiles >= rules.hegemony_tiles[k];
            let held_ok = f.captured_held >= rules.hegemony_cities[k];
            // Tier 3 is reached by territory *or* by holding a conquest; the others need both.
            if tier == 3 {
                tiles_ok || (rules.hegemony_cities[k] > 0 && held_ok)
            } else {
                tiles_ok && held_ok
            }
        }
        1 => f.pop >= rules.prosperity_pop[k] && f.wealth >= rules.prosperity_wealth[k] as u64,
        2 => match tier {
            1..=3 => f.techs >= rules.science_techs[k],
            4 => f.star_gate_max >= 1,
            _ => f.star_gate_max >= 3,
        },
        _ => {
            let partners = f.partners >= rules.concord_partners[k];
            let suz = f.suzerainties >= rules.concord_suzerains[k];
            match tier {
                1 => partners || f.envoy_sent,
                2 => partners && f.ever_suzerain,
                3 => partners && suz,
                4 => partners && suz && f.alliances >= 1 && f.trade >= rules.concord_trade[0] as u64,
                _ => partners && suz && f.trade >= rules.concord_trade[1] as u64,
            }
        }
    }
}

/// Tier reached on each path (0 = none).
pub fn tiers(rules: &Ruleset, f: &Facts) -> [u8; 4] {
    core::array::from_fn(|p| (1..=5).take_while(|t| milestone(rules, f, p, *t)).count() as u8)
}

/// Era: the highest tier reached on at least two paths (three for tier 5).
pub fn era(tiers: [u8; 4]) -> u8 {
    let mut e = 0;
    for k in 1..=5u8 {
        let need = if k == 5 { 3 } else { 2 };
        if tiers.iter().filter(|t| **t >= k).count() >= need {
            e = k;
        } else {
            break;
        }
    }
    e
}

/// A nation's achievement points (V5 §6.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NationScore {
    pub tiers: [u8; 4],
    pub era: u8,
    pub path_points: [u64; 4],
    pub era_points: u64,
}

impl NationScore {
    pub fn total(&self) -> u64 {
        self.path_points.iter().sum::<u64>() + self.era_points
    }
}

/// Points of a ladder climbed to `tier`: 10 + 20 + … (V5 §6.4).
pub fn ladder_points(rules: &Ruleset, tier: u8) -> u64 {
    rules.tier_points.iter().take(tier as usize).map(|p| *p as u64).sum()
}

/// Points of every civ now.
pub fn nation_scores(state: &WorldState, rules: &Ruleset) -> alloc::vec::Vec<NationScore> {
    facts_all(state, rules).iter().map(|f| score_of(rules, f)).collect()
}

pub fn score_of(rules: &Ruleset, f: &Facts) -> NationScore {
    let t = tiers(rules, f);
    let e = era(t);
    NationScore {
        tiers: t,
        era: e,
        path_points: t.map(|x| ladder_points(rules, x)),
        era_points: ladder_points(rules, e),
    }
}

/// Science ranking key for displays: more stages, then earlier completion,
/// then more cumulative science. Sorts ascending = best first.
pub fn science_key(civ: &crate::state::Civ) -> (core::cmp::Reverse<u8>, u16, core::cmp::Reverse<u64>) {
    (
        core::cmp::Reverse(civ.scores.star_gate_stages),
        civ.scores.star_gate_tick.unwrap_or(u16::MAX),
        core::cmp::Reverse(civ.scores.science_total),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Preset;

    #[test]
    fn era_needs_two_paths_and_three_at_the_top() {
        assert_eq!(era([0, 0, 0, 0]), 0);
        assert_eq!(era([5, 0, 0, 0]), 0);
        assert_eq!(era([1, 1, 0, 0]), 1);
        assert_eq!(era([3, 2, 1, 0]), 2);
        assert_eq!(era([5, 5, 4, 0]), 4);
        assert_eq!(era([5, 5, 5, 0]), 5);
    }

    #[test]
    fn points_grow_by_tier_and_cap_at_1125() {
        let r = Ruleset::new(Preset::Blitz);
        assert_eq!(ladder_points(&r, 0), 0);
        assert_eq!(ladder_points(&r, 1), 10);
        assert_eq!(ladder_points(&r, 3), 65);
        assert_eq!(ladder_points(&r, 5), 225);
        let max = NationScore { tiers: [5; 4], era: 5, path_points: [225; 4], era_points: 225 };
        assert_eq!(max.total(), 1_125);
    }

    #[test]
    fn one_counterparty_counts_up_to_forty_percent() {
        let r = Ruleset::new(Preset::Blitz);
        assert_eq!(trade_effective(&r, &[1000, 0, 0]), 400);
        assert_eq!(trade_effective(&r, &[300, 300, 400]), 1000);
        assert_eq!(trade_effective(&r, &[800, 100, 100]), 600);
        assert_eq!(trade_effective(&r, &[]), 0);
    }
}
