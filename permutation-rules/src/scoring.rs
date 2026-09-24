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

/// Tracks in the order used for tie-breaks between tracks (§14.5).
pub const TRACKS: [&str; 3] = ["dominion", "science", "concord"];

/// Full ranking of every civ on each track, best first. Ties go to the lower
/// civ id (entry order), so the result is total and deterministic.
pub fn rankings(rules: &Ruleset, state: &WorldState) -> [Vec<CivId>; 3] {
    let ids = || (0..state.civs.len() as CivId).collect::<Vec<_>>();
    let mut dom = ids();
    dom.sort_by_key(|&c| (core::cmp::Reverse(state.civs[c as usize].scores.dominion), c));
    let mut sci = ids();
    sci.sort_by_key(|&c| (science_key(&state.civs[c as usize]), c));
    let mut con = ids();
    con.sort_by_key(|&c| (core::cmp::Reverse(concord_final(rules, &state.civs[c as usize])), c));
    [dom, sci, con]
}

/// Final prize distribution (§14.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payouts {
    /// Amount per civ (index = civ id), in the pool's unit (USDC base units).
    pub per_civ: Vec<u64>,
    /// Paid winners per track, best first: (civ, amount).
    pub tracks: [Vec<(CivId, u64)>; 3],
    /// Participation recipients.
    pub participation: Vec<CivId>,
    /// Integer remainders, unclaimed track pools and the participation excess.
    pub rollover: u64,
}

/// Split `pool` by §14.5. Prize coalitions (§14.4) are not registrable yet,
/// so every civ is its own entry.
///
/// * Each track gets `pool × track_share_bps / 10000`; its `N` winners
///   (`winners_count`) share it by geometric weights.
/// * One top-3 per entry: an entry placed top-3 on several tracks keeps the
///   highest-paying one (ties: Dominion, Science, Concord) and is removed from
///   the other tracks, whose placements are recomputed; repeated until stable.
/// * Participation: an equal split among entries that ordered in ≥ 60% of
///   their ticks and ranked in the top half of some track, capped at
///   2 × entry fee each.
pub fn payouts(rules: &Ruleset, state: &WorldState, pool: u64) -> Payouts {
    let n = state.civs.len();
    let full = rankings(rules, state);
    let track_pool = |t: usize| (pool as u128 * rules.track_share_bps[t] as u128 / BPS_ONE as u128) as u64;
    let mut excluded: [Vec<CivId>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let place = |excluded: &[Vec<CivId>; 3]| -> [Vec<(CivId, u64)>; 3] {
        core::array::from_fn(|t| {
            let eligible: Vec<CivId> = full[t].iter().copied().filter(|c| !excluded[t].contains(c)).collect();
            let k = winners_count(rules, n).min(eligible.len());
            let (shares, _) = split_pool(track_pool(t), &payout_weights(rules, k));
            eligible.into_iter().zip(shares).collect()
        })
    };
    let mut tracks = place(&excluded);
    for _ in 0..3 {
        let mut changed = false;
        for civ in 0..n as CivId {
            // (amount, -track) so max() prefers the higher amount, then the earlier track.
            let top3: Vec<(u64, core::cmp::Reverse<usize>)> = (0..3)
                .filter_map(|t| tracks[t].iter().take(3).find(|(c, _)| *c == civ).map(|(_, a)| (*a, core::cmp::Reverse(t))))
                .collect();
            if top3.len() < 2 {
                continue;
            }
            let keep = top3.iter().max().unwrap().1 .0;
            for (_, core::cmp::Reverse(t)) in top3 {
                if t != keep && !excluded[t].contains(&civ) {
                    excluded[t].push(civ);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
        tracks = place(&excluded);
    }
    let mut per_civ = alloc::vec![0u64; n];
    let mut paid = 0u64;
    for list in &tracks {
        for (c, a) in list {
            per_civ[*c as usize] += a;
            paid += a;
        }
    }
    // Participation (§14.5).
    let half = n.div_ceil(2);
    let participation: Vec<CivId> = state
        .civs
        .iter()
        .filter(|c| {
            let ticks = state.tick.saturating_sub(c.joined_tick) as u64;
            let active = c.active_ticks as u64 * BPS_ONE as u64 >= 6_000 * ticks && ticks > 0;
            let ranked = full.iter().any(|r| r.iter().take(half).any(|x| *x == c.id));
            active && ranked
        })
        .map(|c| c.id)
        .collect();
    let part_pool = track_pool(3);
    if !participation.is_empty() {
        let each = (part_pool / participation.len() as u64).min(2 * rules.entry_fee_usdc);
        for c in &participation {
            per_civ[*c as usize] += each;
            paid += each;
        }
    }
    Payouts { per_civ, tracks, participation, rollover: pool - paid }
}

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
