//! Victory tracks and payout arithmetic (§14). All scores are cumulative
//! integer sums over ticks and never decrease (§17 invariant 6).

use crate::fixed::BPS_ONE;
use crate::map::Map;
use crate::params::Ruleset;
use crate::state::{Civ, CivId, WorldState};
use alloc::vec::Vec;

/// Dominion contribution of one tick (§14.1).
pub fn dominion_tick(state: &WorldState, rules: &Ruleset, civ: CivId) -> u64 {
    let tiles = owned_tile_value(&state.map, state, civ);
    let captured: u64 = state
        .living_cities_of(civ)
        .filter(|c| captured_city_scores(state, rules, c, civ))
        .map(|c| 5 * c.pop as u64)
        .sum();
    tiles + captured
}

fn owned_tile_value(map: &Map, state: &WorldState, civ: CivId) -> u64 {
    map.tiles
        .iter()
        .filter(|t| {
            t.owner_city
                .and_then(|id| state.cities.get(id as usize))
                .is_some_and(|c| c.alive && c.owner == Some(civ))
        })
        .map(|t| 1 + t.resource.is_some() as u64)
        .sum()
}

/// Eligibility of a captured city for Dominion points this tick (§14.1).
///
/// `capture_scores` is decided at capture (founder and founded-age rules) and
/// cleared in phase 10 if this captor already scored the city this season.
/// TODO(§14.4): exclude captures from a member of the captor's prize coalition.
pub fn captured_city_scores(
    state: &WorldState,
    rules: &Ruleset,
    city: &crate::state::City,
    captor: CivId,
) -> bool {
    let Some(captured) = city.captured_tick else {
        return false;
    };
    city.capture_scores
        && city.owner == Some(captor)
        && state.tick.saturating_sub(captured) + 1 >= rules.capture_hold_ticks
}

/// Concord contribution of one tick (§14.3). `max_pop_before` is the civ's
/// highest total population before this tick.
pub fn concord_tick(
    state: &WorldState,
    rules: &Ruleset,
    civ: &Civ,
    total_pop: u32,
    max_pop_before: u32,
) -> u64 {
    if civ.is_aggressor(state.tick, rules.aggressor_window) {
        return 0;
    }
    let new_highs = total_pop.saturating_sub(max_pop_before) as u64;
    let suzerain = state
        .city_states
        .iter()
        .filter(|cs| cs.suzerain == Some(civ.id))
        .count() as u64;
    total_pop as u64 + 10 * new_highs + 5 * suzerain
}

/// Final Concord with the neutrality multiplier (§14.3).
pub fn concord_final(rules: &Ruleset, civ: &Civ) -> u64 {
    let m = if civ.ever_allied {
        BPS_ONE
    } else {
        rules.neutrality_mult_bps
    };
    civ.scores.concord_raw * m as u64 / BPS_ONE as u64
}

/// Science ranking key (§14.2): more stages, then earlier completion, then
/// more cumulative science. Sorts ascending = best first.
pub fn science_key(civ: &Civ) -> (core::cmp::Reverse<u8>, u16, core::cmp::Reverse<u64>) {
    (
        core::cmp::Reverse(civ.scores.star_gate_stages),
        civ.scores.star_gate_tick.unwrap_or(u16::MAX),
        core::cmp::Reverse(civ.scores.science_total),
    )
}

/// `N = clamp(entrants × winners_bps / 10000, min, max)` (§14.5).
pub fn winners_count(rules: &Ruleset, entrants: usize) -> usize {
    (entrants * rules.winners_bps as usize / BPS_ONE as usize)
        .clamp(rules.winners_min as usize, rules.winners_max as usize)
        .min(entrants)
}

/// Geometric payout weights: `w_1 = 10000`, `w_{k+1} = w_k × decay / 10000` (§14.5).
pub fn payout_weights(rules: &Ruleset, n: usize) -> Vec<u64> {
    let mut w = Vec::with_capacity(n);
    let mut cur = BPS_ONE as u64;
    for _ in 0..n {
        w.push(cur);
        cur = cur * rules.payout_decay_bps as u64 / BPS_ONE as u64;
    }
    w
}

/// Split `pool` by weights; integer remainder is returned for rollover.
pub fn split_pool(pool: u64, weights: &[u64]) -> (Vec<u64>, u64) {
    let total: u64 = weights.iter().sum();
    if total == 0 {
        return (Vec::new(), pool);
    }
    let shares: Vec<u64> = weights
        .iter()
        .map(|w| (pool as u128 * *w as u128 / total as u128) as u64)
        .collect();
    let paid: u64 = shares.iter().sum();
    (shares, pool - paid)
}

// TODO(§14.4): prize coalitions. TODO(§14.5): the one-top-3-per-entry
// iteration and Participation eligibility; both need the finalized split,
// which is ON HOLD (V4 §12).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Preset;

    #[test]
    fn winners_count_clamps() {
        let r = Ruleset::new(Preset::Blitz);
        assert_eq!(winners_count(&r, 6), 3);
        assert_eq!(winners_count(&r, 100), 20);
        assert_eq!(winners_count(&r, 1000), 50);
        assert_eq!(winners_count(&r, 2), 2);
    }

    #[test]
    fn payout_split_conserves_value() {
        let r = Ruleset::new(Preset::Blitz);
        let w = payout_weights(&r, 5);
        assert_eq!(w, [10_000, 7_500, 5_625, 4_218, 3_163]);
        let (shares, rest) = split_pool(1_000_000_007, &w);
        assert_eq!(shares.iter().sum::<u64>() + rest, 1_000_000_007);
        assert!(shares.windows(2).all(|p| p[0] >= p[1]));
    }
}
