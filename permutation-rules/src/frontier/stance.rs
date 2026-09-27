//! Stances and postures (design §6.3).
//!
//! | Matchup | Effect |
//! |---|---|
//! | Assault > Flank > Brace > Assault | the winner deals +20% damage |
//! | Hold (default doctrine, public) | no modifier either way |
//! | Disarray (committed, not revealed) | deals ×0.6, takes ×1.25 |
//!
//! An arrival's stance is part of its sealed march commitment, so a
//! revealed arrival always has one. A defending host or holding that
//! commits no posture before the bell stands in Hold; one that commits
//! and does not reveal by the reveal close is in Disarray, which is
//! strictly worse than revealing even a losing stance (no free last look,
//! design §6.3 and `rev2-results.txt` §B).

/// Version of this kernel, bound into `RULESET_HASH` (`super::KERNEL_VERSIONS`):
/// bump it whenever an honest outcome changes. v1: as at M0; the damage table is also hashed.
pub const STANCE_VERSION: u16 = 1;

use crate::fixed::{Bps, BPS_ONE};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
pub enum Stance {
    Hold,
    Assault,
    Flank,
    Brace,
}

pub const STANCES: [Stance; 4] = [Stance::Hold, Stance::Assault, Stance::Flank, Stance::Brace];

/// What a combatant fights with at the bell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum Posture {
    Stance(Stance),
    /// Committed and not revealed by the reveal close.
    Disarray,
}

impl Default for Posture {
    fn default() -> Self {
        Posture::Stance(Stance::Hold)
    }
}

/// The cycle winner deals this much more.
pub const WIN_BONUS_BPS: Bps = 12_000;
/// Disarray deals ×0.6 …
pub const DISARRAY_DEALT_BPS: Bps = 6_000;
/// … and takes ×1.25.
pub const DISARRAY_TAKEN_BPS: Bps = 12_500;

impl Stance {
    /// `self` wins the 3-cycle against `other`.
    pub const fn beats(self, other: Stance) -> bool {
        use Stance::*;
        matches!(
            (self, other),
            (Assault, Flank) | (Flank, Brace) | (Brace, Assault)
        )
    }

    pub const fn from_u8(v: u8) -> Option<Stance> {
        match v {
            0 => Some(Stance::Hold),
            1 => Some(Stance::Assault),
            2 => Some(Stance::Flank),
            3 => Some(Stance::Brace),
            _ => None,
        }
    }
}

/// A defender's posture at the bell: no commitment → Hold; committed and
/// revealed → that stance; committed and not revealed → Disarray.
pub const fn posture_of(committed: bool, revealed: Option<Stance>) -> Posture {
    match (committed, revealed) {
        (false, _) => Posture::Stance(Stance::Hold),
        (true, Some(s)) => Posture::Stance(s),
        (true, None) => Posture::Disarray,
    }
}

/// Multiplier (bps) on the damage a combatant in posture `att` deals to one
/// in posture `def`.
pub const fn damage_bps(att: Posture, def: Posture) -> Bps {
    let mut m = BPS_ONE as u64;
    match (att, def) {
        (Posture::Stance(a), Posture::Stance(d)) if a.beats(d) => {
            m = m * WIN_BONUS_BPS as u64 / BPS_ONE as u64;
        }
        _ => {}
    }
    if matches!(att, Posture::Disarray) {
        m = m * DISARRAY_DEALT_BPS as u64 / BPS_ONE as u64;
    }
    if matches!(def, Posture::Disarray) {
        m = m * DISARRAY_TAKEN_BPS as u64 / BPS_ONE as u64;
    }
    m as Bps
}

/// Damage-exchange payoff of `a` against `b` for equal forces, in bps:
/// `(dealt / taken − 1) × 10000`. Zero-sum in sign, 0 for a neutral pair.
pub const fn payoff_bps(a: Posture, b: Posture) -> i64 {
    let dealt = damage_bps(a, b) as i64;
    let taken = damage_bps(b, a) as i64;
    dealt * BPS_ONE as i64 / taken - BPS_ONE as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_is_a_three_cycle_with_hold_neutral() {
        for a in STANCES {
            for b in STANCES {
                let n = [a.beats(b), b.beats(a)].iter().filter(|x| **x).count();
                let expect = a != b && a != Stance::Hold && b != Stance::Hold;
                assert_eq!(n == 1, expect, "{a:?} {b:?}");
            }
        }
    }

    /// CL-14: the clash's damage bound is pinned by value. `MAX_STANCE_BPS`
    /// is computed from this table, so comparing the table with it would be
    /// a tautology; the literals below force a conscious review of CL-14
    /// (and of the const asserts in `clash`) whenever a stance multiplier
    /// or the doctrine ceiling changes. The largest pairing is a stance
    /// against Disarray, ×1.25.
    #[test]
    fn the_stance_table_is_inside_the_damage_bound() {
        use crate::frontier::clash::{MAX_DAMAGE_PRODUCT_BPS, MAX_STANCE_BPS};
        assert_eq!(MAX_STANCE_BPS, 12_500);
        assert_eq!(MAX_STANCE_BPS, DISARRAY_TAKEN_BPS);
        assert_eq!(MAX_DAMAGE_PRODUCT_BPS, 14_375);
    }
}
