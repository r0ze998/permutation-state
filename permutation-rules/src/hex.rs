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

    /// Turned by 60° about the origin (the map's rotation, `mapgen`).
    pub const fn rotate(self) -> Hex {
        Hex::new(-self.r, self.q + self.r)
    }

    /// Turned by −60°.
    pub const fn unrotate(self) -> Hex {
        Hex::new(self.q + self.r, -self.q)
    }

    /// Turned by `k` × 60°.
    pub fn rotate_by(self, k: u8) -> Hex {
        let mut h = self;
        for _ in 0..k % 6 {
            h = h.rotate();
        }
        h
    }

    /// The sextant of this hex: the `k` with `self == w.rotate_by(k)` for a
    /// `w` with `q > 0, r ≥ 0` (0 for the origin).
    pub fn sextant(self) -> u8 {
        let mut w = self;
        for k in 0..6u8 {
            if w.q > 0 && w.r >= 0 {
                return k;
            }
            w = w.unrotate();
        }
        0
    }

    /// `self` seen from sextant `k` (turned back by `k` × 60°). Rules that
    /// break a tie by hex order compare these instead of raw hexes, so the
    /// choice turns with the map: on a symmetric map every position is
    /// treated alike.
    pub fn turned(self, k: u8) -> Hex {
        self.rotate_by((6 - k % 6) % 6)
    }

    /// Neighbours in §0.3 order turned to sextant `k` (the order a rule that
    /// takes "the first free neighbour" uses, so that it turns with the map).
    pub fn neighbors_in(self, k: u8) -> [Hex; 6] {
        core::array::from_fn(|j| {
            let (dq, dr) = DIRECTIONS[(j + 6 - (k % 6) as usize) % 6];
            Hex::new(self.q + dq, self.r + dr)
        })
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

    /// The turned neighbour order and tie keys turn with the map.
    #[test]
    fn turned_orders_follow_the_rotation() {
        for h in hexes_within(6) {
            let k = h.sextant();
            let r = h.rotate();
            if h != Hex::ORIGIN {
                assert_eq!(r.sextant(), (k + 1) % 6);
                let a = h.neighbors_in(k);
                let b = r.neighbors_in(r.sextant());
                for j in 0..6 {
                    assert_eq!(a[j].rotate(), b[j]);
                }
                for o in hexes_within(3) {
                    assert_eq!(o.turned(k), o.rotate().turned(r.sextant()));
                }
            }
            assert_eq!(h.rotate_by(6), h);
            assert_eq!(h.rotate().unrotate(), h);
        }
    }

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
