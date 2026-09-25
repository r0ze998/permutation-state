//! The history layer (revised 2026-09-25): what a season leaves to the next.
//!
//! Terrain, stocks and strength never carry over — every season starts on a
//! new symmetric map, so the prize stays fair. What carries over is the
//! record: how each nation ended (points, era, milestone tiers, share of the
//! pool, cities), every city (where it stood, who founded it, who held it at
//! the end, whom it was taken from), and the ruins left behind. The chain
//! keeps it as a hash chain: `history_root = sha256("permutation-rules/
//! history" ‖ previous root ‖ sha256(borsh(record)))`, written when a season
//! is finalized and mixed into the next season's seed, so anyone can check
//! that a season's history follows from its predecessor's final world and
//! that the next season was built on it. The record is never used by the
//! rules or the payouts.

use crate::hex::Hex;
use crate::payout::Settlement;
use crate::state::{CivId, WorldState};
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NationRecord {
    pub points: u64,
    pub era: u8,
    pub tiers: [u8; 4],
    /// USDC the nation's members shared (0 if it was not counted).
    pub share: u64,
    pub cities: u32,
    pub members: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CityRecord {
    pub hex: Hex,
    pub founder: CivId,
    pub founded_tick: u16,
    /// The holder at the end (`None`: a Free City or a ruin).
    pub owner: Option<CivId>,
    pub captured_from: Option<CivId>,
    pub pop: u32,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SeasonRecord {
    /// The final world's state root.
    pub final_root: [u8; 32],
    pub nations: Vec<NationRecord>,
    pub cities: Vec<CityRecord>,
    /// Ruins left on the map: (hex, peak population).
    pub ruins: Vec<(Hex, u16)>,
}

/// The record of a finished season, from its final world and settlement.
pub fn season_record(state: &WorldState, settlement: &Settlement) -> SeasonRecord {
    let final_root = state.state_root().unwrap_or([0; 32]);
    let nations = (0..state.civs.len())
        .map(|c| {
            let score = settlement.scores.get(c).copied().unwrap_or_default();
            NationRecord {
                points: score.total(),
                era: score.era,
                tiers: score.tiers,
                share: settlement.nation_share.get(c).copied().unwrap_or(0),
                cities: state.city_count(c as CivId),
                members: state.nations.get(c).map_or(0, |n| n.members),
            }
        })
        .collect();
    let cities = state
        .cities
        .iter()
        .map(|c| CityRecord {
            hex: c.hex,
            founder: c.founder,
            founded_tick: c.founded_tick,
            owner: if c.alive { c.owner } else { None },
            captured_from: c.captured_from,
            pop: c.pop,
            alive: c.alive,
        })
        .collect();
    let ruins = state
        .map
        .tiles
        .iter()
        .filter_map(|t| t.ruin_peak_pop.map(|p| (t.hex, p)))
        .collect();
    SeasonRecord {
        final_root,
        nations,
        cities,
        ruins,
    }
}

/// The next link of the history chain.
pub fn history_root(prev: &[u8; 32], record: &SeasonRecord) -> [u8; 32] {
    let bytes = borsh::to_vec(record).expect("borsh into a Vec cannot fail");
    let digest = crate::hash::sha256(&[&bytes]);
    crate::hash::sha256(&[b"permutation-rules/history", prev, &digest])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::{nation_entries, new_season};
    use crate::params::{Preset, Ruleset};
    use crate::payout::settle;

    #[test]
    fn the_record_follows_the_world_and_chains_the_roots() {
        let rules = Ruleset::new(Preset::Blitz);
        let mut s = new_season(&rules, &[1; 32], &[2; 32], &nation_entries(6)).unwrap();
        let settlement = settle(&s, &rules, 0, 10);
        let r = season_record(&s, &settlement);
        assert_eq!(r.nations.len(), 6);
        assert_eq!(r.cities.len(), 6);
        assert!(r
            .cities
            .iter()
            .all(|c| c.alive && c.owner == Some(c.founder)));
        assert_eq!(r.final_root, s.state_root().unwrap());
        let a = history_root(&[0; 32], &r);
        assert_ne!(
            a,
            history_root(&[1; 32], &r),
            "the previous root is part of it"
        );
        s.cities[0].pop += 1;
        let r2 = season_record(&s, &settle(&s, &rules, 0, 10));
        assert_ne!(history_root(&[0; 32], &r2), a, "the world is part of it");
    }
}
