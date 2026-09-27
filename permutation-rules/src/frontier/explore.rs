//! Exploration finds (contract §7, I-56; `frontier-sim` `sim.rs` l.
//! 1370–1380).
//!
//! An exploration gives `WORKS_EXPLORE` (4 Works) **always** while it is
//! one of the holding's three floor explorations, otherwise with chance
//! ½. There are no goods in M1 (the simulator has none; DESIGN §2.3's
//! goods floor waits for M2).
//!
//! **Draw** (pinned): `x = rng::rand(seed, "frontier/explore", le32(P) ‖
//! le32(Q) ‖ [tile] ‖ le64(host))`; a non-floor exploration finds iff
//! `camp::below(x, 2) == 0`. `seed` is the bell seed SettleExplore uses.

use crate::rng::rand;

use super::camp::below;
use super::catalog::WORKS_EXPLORE;
use super::geometry::ProvinceCoord;

/// Kernel version of this module (part of the ruleset hash).
pub const EXPLORE_VERSION: u16 = 1;

/// Floor explorations per holding (`Citizen.explores_floor_left` starts
/// here).
pub const EXPLORE_FLOOR: u8 = 3;

/// What an exploration found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Find {
    pub works: u32,
}

/// The find of host `host` exploring `tile` of province `p` under `seed`.
pub fn roll(seed: &[u8; 32], p: ProvinceCoord, tile: u8, host: u64, floor: bool) -> Find {
    if floor {
        return Find {
            works: WORKS_EXPLORE as u32,
        };
    }
    let mut id = [0u8; 17];
    id[..4].copy_from_slice(&p.p.to_le_bytes());
    id[4..8].copy_from_slice(&p.q.to_le_bytes());
    id[8] = tile;
    id[9..17].copy_from_slice(&host.to_le_bytes());
    let x = rand(seed, b"frontier/explore", &id);
    Find {
        works: if below(x, 2) == 0 {
            WORKS_EXPLORE as u32
        } else {
            0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_always_finds_and_the_rest_half() {
        let p = ProvinceCoord::new(3, -1);
        let mut found = 0;
        for host in 0..2_000u64 {
            let seed = [(host % 251) as u8; 32];
            assert_eq!(roll(&seed, p, 7, host, true).works, 4);
            let f = roll(&seed, p, (host % 61) as u8, host, false);
            assert!(f.works == 0 || f.works == 4);
            found += (f.works == 4) as u32;
            assert_eq!(f, roll(&seed, p, (host % 61) as u8, host, false));
        }
        assert!((850..=1_150).contains(&found), "{found}");
    }
}
