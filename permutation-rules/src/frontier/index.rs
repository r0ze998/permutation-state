//! The per-capita faction index with herding damping (open-world design
//! §5.6, revision 2).
//!
//! For each path p (Dominion, Prosperity, Knowledge, Concord) a faction's
//! value **per active member** is divided by the civilization's value per
//! active member. The index is the mean of the four ratios, clamped to
//! [0.5, 2], times the herding damping `h_k = min(1, (1/6) / share_k)^γ`
//! with γ = 0.6 by default:
//!
//! ```text
//! s_k = clamp(mean_p(ratio_p), 0.5, 2) × h_k
//! ```
//!
//! The citizen pool is split among factions by `fee_k × √s_k` and the
//! laurel pool by `stake_k × s_k` (§5.4); the weight helpers are here so the
//! program, the verifier and the join page's live "prize per citizen if the
//! season ended now" all use one formula.
//!
//! Fixed point: `INDEX_ONE` = 1.0. No floating point; `pow_frac` is a
//! deterministic binary search over integer powers, so every platform gets
//! the same bits.

use super::pools::EconError;
use crate::fixed::isqrt;
use borsh::{BorshDeserialize, BorshSerialize};

/// Factions in a Frontier season.
pub const FACTIONS: usize = 6;
/// Scoring paths: Dominion, Prosperity, Knowledge, Concord.
pub const PATHS: usize = 4;
/// 1.0 in index fixed point.
pub const INDEX_ONE: u64 = 1_000_000;

/// Scoring paths in `FactionFacts::path` order (§5.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    /// March-control bells plus captures.
    Dominion = 0,
    /// Production.
    Prosperity = 1,
    /// Techs researched and science pledged.
    Knowledge = 2,
    /// Engine contribution and treaty-days kept (no Bourse volume).
    Concord = 3,
}

/// What `FoldFaction` reads from the faction's 16 shards (after Shade
/// voiding, `remove` each revealed Shade's own facts).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FactionFacts {
    /// Paid citizens (the herding share's base).
    pub members: u64,
    /// Term-active citizens (the per-capita base).
    pub active: u64,
    /// Path totals, indexed by `Path`.
    pub path: [u64; PATHS],
}

impl FactionFacts {
    /// Add one citizen's (or one shard's) facts.
    pub fn add(&mut self, d: &FactionFacts) -> Result<(), EconError> {
        let mut n = *self;
        n.members = n
            .members
            .checked_add(d.members)
            .ok_or(EconError::Overflow)?;
        n.active = n.active.checked_add(d.active).ok_or(EconError::Overflow)?;
        for p in 0..PATHS {
            n.path[p] = n.path[p]
                .checked_add(d.path[p])
                .ok_or(EconError::Overflow)?;
        }
        *self = n;
        Ok(())
    }

    /// Subtract a revealed Shade's recorded facts (§7.3).
    pub fn remove(&mut self, d: &FactionFacts) -> Result<(), EconError> {
        let mut n = *self;
        n.members = n
            .members
            .checked_sub(d.members)
            .ok_or(EconError::Underflow)?;
        n.active = n.active.checked_sub(d.active).ok_or(EconError::Underflow)?;
        for p in 0..PATHS {
            n.path[p] = n.path[p]
                .checked_sub(d.path[p])
                .ok_or(EconError::Underflow)?;
        }
        *self = n;
        Ok(())
    }
}

/// The index's tunables. γ is a fraction `gamma_num / gamma_den` so it can
/// be re-tuned after the M0 β measurement without floating point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct IndexParams {
    pub clamp_lo: u64,
    pub clamp_hi: u64,
    pub gamma_num: u32,
    pub gamma_den: u32,
}

impl IndexParams {
    /// Revision 2: clamp [0.5, 2], γ = 0.6.
    pub const REV2: IndexParams = IndexParams {
        clamp_lo: INDEX_ONE / 2,
        clamp_hi: 2 * INDEX_ONE,
        gamma_num: 3,
        gamma_den: 5,
    };
}

impl Default for IndexParams {
    fn default() -> Self {
        Self::REV2
    }
}

/// `(fact_k / active_k) / (fact_all / active_all)` in `INDEX_ONE` units,
/// saturating at `cap`. A path nobody scored in is neutral (1.0); a faction
/// with no active member scores 0 on every scored path.
pub fn per_capita_ratio(
    fact_k: u64,
    active_k: u64,
    fact_all: u64,
    active_all: u64,
    cap: u64,
) -> u64 {
    if fact_all == 0 || active_all == 0 {
        return INDEX_ONE.min(cap);
    }
    if active_k == 0 {
        return 0;
    }
    let num = (fact_k as u128)
        .checked_mul(active_all as u128)
        .and_then(|x| x.checked_mul(INDEX_ONE as u128));
    let den = active_k as u128 * fact_all as u128; // < 2^128: both are u64
    match num {
        Some(n) => (n / den).min(cap as u128) as u64,
        None => cap,
    }
}

/// Internal precision of `pow_frac`: 1.0 = 10^18.
const POW_ONE: u128 = 1_000_000_000_000_000_000;

/// `v^n` for `v ≤ INDEX_ONE`, computed at 10^18 precision and rounded down
/// at each step (so it is monotone in `v`).
fn pow_fix(v: u64, n: u32) -> u128 {
    let v = v as u128 * (POW_ONE / INDEX_ONE as u128);
    let mut acc = POW_ONE;
    for _ in 0..n {
        acc = acc * v / POW_ONE; // ≤ 10^36 before the division
    }
    acc
}

/// `x^(num/den)` for `0 ≤ x ≤ INDEX_ONE` and `num ≤ den`: the largest `y`
/// (in `INDEX_ONE` units) with `y^den ≤ x^num`, by 20 halvings of [0, 1].
pub fn pow_frac(x: u64, num: u32, den: u32) -> u64 {
    let x = x.min(INDEX_ONE);
    if den == 0 || num == 0 {
        return INDEX_ONE;
    }
    if x == 0 {
        return 0;
    }
    let target = pow_fix(x, num);
    let (mut lo, mut hi) = (0u64, INDEX_ONE);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if pow_fix(mid, den) <= target {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// Herding damping `h_k = min(1, (1/6) / share_k)^γ`, share by paid
/// members. A faction at or below an equal sixth is not damped.
pub fn herding(members_k: u64, members_all: u64, p: &IndexParams) -> u64 {
    if members_k == 0 || members_all == 0 {
        return INDEX_ONE;
    }
    // (1/6) / (m_k / m_all) = m_all / (6 m_k)
    let x = (members_all as u128 * INDEX_ONE as u128 / (FACTIONS as u128 * members_k as u128))
        .min(INDEX_ONE as u128) as u64;
    if x == INDEX_ONE {
        return INDEX_ONE;
    }
    pow_frac(x, p.gamma_num, p.gamma_den)
}

/// The undamped part: `clamp(mean_p(ratio_p), lo, hi)`.
pub fn clamped_mean(k: usize, facts: &[FactionFacts; FACTIONS], p: &IndexParams) -> u64 {
    let active_all: u64 = facts.iter().fold(0u64, |a, f| a.saturating_add(f.active));
    // Any single ratio above PATHS × hi already forces the mean to hi.
    let cap = p.clamp_hi.saturating_mul(PATHS as u64);
    let mut sum = 0u64;
    for path in 0..PATHS {
        let all: u64 = facts
            .iter()
            .fold(0u64, |a, f| a.saturating_add(f.path[path]));
        sum += per_capita_ratio(facts[k].path[path], facts[k].active, all, active_all, cap);
    }
    (sum / PATHS as u64).clamp(p.clamp_lo, p.clamp_hi)
}

/// `s_k` for every faction (`FoldFaction`/`FinalizeFaction`, §5.6). O(6×4).
pub fn faction_index(facts: &[FactionFacts; FACTIONS], p: &IndexParams) -> [u64; FACTIONS] {
    let members_all: u64 = facts.iter().fold(0u64, |a, f| a.saturating_add(f.members));
    let mut s = [0u64; FACTIONS];
    for k in 0..FACTIONS {
        let m = clamped_mean(k, facts, p);
        let h = herding(facts[k].members, members_all, p);
        s[k] = (m as u128 * h as u128 / INDEX_ONE as u128) as u64;
    }
    s
}

/// `√s` in `INDEX_ONE` fixed point (the citizen pool's softening).
pub fn sqrt_index(s: u64) -> u64 {
    isqrt(s.saturating_mul(INDEX_ONE))
}

/// A faction's claim on the citizen pool: `C_k ∝ fee_k × √s_k` (§5.4). The
/// design's `n_k` is read as fee-weighted membership so that a USDC paid
/// on day 21 buys the same faction weight as one paid on day 0.
pub fn citizen_faction_weight(fee_k: u128, s_k: u64) -> Option<u128> {
    fee_k.checked_mul(sqrt_index(s_k) as u128)
}

/// A faction's claim on the laurel pool: `L_k ∝ Σ stake_k × s_k` (§5.4).
pub fn laurel_faction_weight(stake_k: u128, s_k: u64) -> Option<u128> {
    stake_k.checked_mul(s_k as u128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pow_frac_matches_known_values() {
        // 0.5^0.6 = 0.659754, (1/3)^0.6 = 0.517282, 0.25^0.5 = 0.5
        assert!(pow_frac(500_000, 3, 5).abs_diff(659_754) <= 2);
        assert!(pow_frac(333_333, 3, 5).abs_diff(517_282) <= 2);
        assert_eq!(pow_frac(250_000, 1, 2), 500_000);
        assert_eq!(pow_frac(INDEX_ONE, 3, 5), INDEX_ONE);
        assert_eq!(pow_frac(0, 3, 5), 0);
    }

    #[test]
    fn pow_frac_is_monotone() {
        let mut last = 0;
        for x in (0..=INDEX_ONE).step_by(997) {
            let y = pow_frac(x, 3, 5);
            assert!(y >= last && y >= x, "x={x}");
            last = y;
        }
    }
}
