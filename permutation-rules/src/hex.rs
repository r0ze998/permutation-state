//! Axial hex coordinates (§0.3).

use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};

/// Neighbour order E, NE, NW, W, SW, SE (§0.3). Fixed because it affects
/// deterministic iteration.
pub const DIRECTIONS: [(i32, i32); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];

/// Axial coordinate. Ordering is by `q`, then `r`, which is also the "tile
/// index" order used for tie-breaks in the spec.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
pub struct Hex {
    pub q: i32,
    pub r: i32,
}

impl Hex {
    pub const ORIGIN: Hex = Hex { q: 0, r: 0 };

    pub const fn new(q: i32, r: i32) -> Self {
        Hex { q, r }
    }

    pub const fn s(self) -> i32 {
        -self.q - self.r
    }

    pub fn distance(self, other: Hex) -> u32 {
        let dq = (self.q - other.q).unsigned_abs();
        let dr = (self.r - other.r).unsigned_abs();
        let ds = (self.s() - other.s()).unsigned_abs();
        (dq + dr + ds) / 2
    }

    pub fn neighbors(self) -> [Hex; 6] {
        DIRECTIONS.map(|(dq, dr)| Hex::new(self.q + dq, self.r + dr))
    }

    /// Distance from the map origin.
    pub fn radius(self) -> u32 {
        self.distance(Hex::ORIGIN)
    }
}

/// All hexes within `radius` of the origin, sorted by (q, r).
/// Count is `3·R·(R+1) + 1`.
pub fn hexes_within(radius: u32) -> Vec<Hex> {
    let r = radius as i32;
    let mut out = Vec::with_capacity((3 * radius * (radius + 1) + 1) as usize);
    for q in -r..=r {
        let r_min = (-r).max(-q - r);
        let r_max = r.min(-q + r);
        for rr in r_min..=r_max {
            out.push(Hex::new(q, rr));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_match_spec_map_sizes() {
        assert_eq!(hexes_within(13).len(), 547); // Blitz
        assert_eq!(hexes_within(19).len(), 1141); // Season
    }

    #[test]
    fn hexes_are_sorted_and_within_radius() {
        let hs = hexes_within(5);
        assert!(hs.windows(2).all(|w| w[0] < w[1]));
        assert!(hs.iter().all(|h| h.radius() <= 5));
    }

    #[test]
    fn neighbors_are_distance_one() {
        let h = Hex::new(2, -3);
        for n in h.neighbors() {
            assert_eq!(h.distance(n), 1);
        }
    }
}
